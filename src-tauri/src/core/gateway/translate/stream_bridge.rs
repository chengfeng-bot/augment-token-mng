//! 流式桥接：渠道 SSE 字节 → canonical 事件 → 客户端 SSE 字节。
//!
//! 增量解析渠道 SSE 帧，经 `OutboundTranslator` 解析为 canonical 事件，再由
//! `InboundTranslator` 重渲染为客户端协议分片。Responses↔Codex 同线型直转在路由层
//! 短路，不进入本桥接。

use super::anthropic::Anthropic;
use super::openai_chat::OpenAiChat;
use super::openai_responses::OpenAiResponses;
use super::{InboundTranslator, OutboundTranslator, ParseState, RenderState, Wire};
use crate::core::gateway::canonical::{StreamEvent, Usage};
use serde_json::Value;

/// 按线型取入站转换器
pub fn inbound_for(wire: Wire) -> Box<dyn InboundTranslator> {
    match wire {
        Wire::Chat => Box::new(OpenAiChat),
        Wire::Responses => Box::new(OpenAiResponses),
        Wire::Anthropic => Box::new(Anthropic),
    }
}

/// 按线型取出站转换器
pub fn outbound_for(wire: Wire) -> Box<dyn OutboundTranslator> {
    match wire {
        Wire::Chat => Box::new(OpenAiChat),
        Wire::Responses => Box::new(OpenAiResponses),
        Wire::Anthropic => Box::new(Anthropic),
    }
}

/// 已完成的 SSE 帧（含原始字节，供同线型透传原样写出）
#[derive(Debug, Clone, Default)]
pub struct SseFrame {
    pub event: Option<String>,
    pub data: String,
    pub raw: Vec<u8>,
}

impl SseFrame {
    /// 同线型透传：传输层心跳直接丢弃，其余帧保留原始字节
    pub fn passthrough_bytes(&self) -> Vec<u8> {
        if transport_heartbeat_name(self).is_some() {
            return Vec::new();
        }
        if !self.raw.is_empty() {
            return self.raw.clone();
        }
        let mut out = String::new();
        if let Some(event) = &self.event {
            out.push_str("event: ");
            out.push_str(event);
            out.push('\n');
        }
        out.push_str("data: ");
        out.push_str(&self.data);
        out.push_str("\n\n");
        out.into_bytes()
    }
}

/// 增量 SSE 帧解析（以空行分帧，聚合 event/data 行）
///
/// 以字节缓冲累积：既兼容 `\n\n` / `\r\n\r\n` / `\r\r` 三种事件分隔风格，也避免多字节
/// UTF-8 字符被网络分片切断后按 lossy 解码损坏（仅在凑齐完整帧后再解码）。
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    /// 喂入新字节，返回已完成的帧（含注释帧，供透传保留上游 keep-alive 注释）
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseFrame> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        while let Some((pos, sep_len)) = next_boundary(&self.buf) {
            let raw: Vec<u8> = self.buf.drain(..pos + sep_len).collect();
            let text = String::from_utf8_lossy(&raw);
            let mut event = None;
            let mut data = String::new();
            for line in text.split('\n') {
                let line = line.trim_end_matches('\r');
                if let Some(rest) = line.strip_prefix("event:") {
                    event = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                }
            }
            if !data.is_empty() || event.is_some() {
                frames.push(SseFrame { event, data, raw });
            }
        }
        frames
    }
}

fn transport_heartbeat_name(frame: &SseFrame) -> Option<String> {
    if let Some(event) = &frame.event {
        if is_transport_heartbeat_label(event) {
            return Some(normalize_heartbeat_label(event));
        }
    }
    let value: Value = serde_json::from_str(frame.data.trim()).ok()?;
    let kind = value.get("type")?.as_str()?;
    if is_transport_heartbeat_label(kind) {
        Some(normalize_heartbeat_label(kind))
    } else {
        None
    }
}

fn is_transport_heartbeat_label(value: &str) -> bool {
    matches!(
        normalize_heartbeat_label(value).as_str(),
        "keepalive" | "keep-alive" | "ping" | "heartbeat"
    )
}

fn normalize_heartbeat_label(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Default)]
pub struct StreamSanitizer;

impl StreamSanitizer {
    /// 同线型透传：心跳和非应用事件直接丢弃，不转发给客户端
    pub fn passthrough(&self, frame: &SseFrame, wire: Wire) -> Vec<u8> {
        if transport_heartbeat_name(frame).is_some() {
            return Vec::new();
        }
        if wire == Wire::Responses {
            if let Some(kind) = frame_event_type(frame) {
                if !is_responses_application_type(&kind) {
                    return Vec::new();
                }
            }
        }
        frame.passthrough_bytes()
    }
}

fn frame_event_type(frame: &SseFrame) -> Option<String> {
    if let Some(event) = &frame.event {
        let event = event.trim();
        if !event.is_empty() {
            return Some(event.to_string());
        }
    }
    let value: Value = serde_json::from_str(frame.data.trim()).ok()?;
    value
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn is_responses_application_type(kind: &str) -> bool {
    let kind = kind.trim();
    kind == "error" || kind.starts_with("response.")
}

/// 在字节缓冲中查找子串位置
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// 定位下一个 SSE 事件边界（空行），兼容 `\n\n` / `\r\n\r\n` / `\r\r`，
/// 返回 (起始偏移, 分隔符长度)；取最靠前的边界
fn next_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    let seps: [(&[u8], usize); 3] = [(b"\r\n\r\n", 4), (b"\n\n", 2), (b"\r\r", 2)];
    seps.iter()
        .filter_map(|&(sep, len)| find_subslice(buf, sep).map(|pos| (pos, len)))
        .min_by_key(|&(pos, _)| pos)
}

/// 跨协议流式桥接器
pub struct StreamBridge {
    out_tr: Box<dyn OutboundTranslator>,
    in_tr: Box<dyn InboundTranslator>,
    decoder: SseDecoder,
    parse: ParseState,
    render: RenderState,
    done: bool,
    usage: Usage,
    error: Option<String>,
}

impl StreamBridge {
    pub fn new(channel_wire: Wire, client_wire: Wire, model: &str) -> Self {
        Self {
            out_tr: outbound_for(channel_wire),
            in_tr: inbound_for(client_wire),
            decoder: SseDecoder::default(),
            parse: ParseState {
                model: model.to_string(),
                ..Default::default()
            },
            render: RenderState {
                model: model.to_string(),
                ..Default::default()
            },
            done: false,
            usage: Usage::default(),
            error: None,
        }
    }

    /// 是否已收到终止事件
    pub fn done(&self) -> bool {
        self.done
    }

    /// 桥接过程中累积的用量（取自上游 Usage 事件，缺失则回退解析状态）
    pub fn usage(&self) -> Usage {
        if self.usage != Usage::default() {
            self.usage.clone()
        } else {
            self.parse.usage.clone()
        }
    }

    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    /// 喂入渠道字节，返回应转发给客户端的字节
    pub fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(bytes) {
            if transport_heartbeat_name(&frame).is_some() {
                continue;
            }
            let events = self.out_tr.parse_stream(
                frame.event.as_deref(),
                &frame.data,
                &mut self.parse,
            );
            for cev in events {
                if let StreamEvent::Usage(u) = &cev {
                    self.usage = u.clone();
                }
                if let StreamEvent::Error { message } = &cev {
                    self.error = Some(message.clone());
                    self.done = true;
                }
                if matches!(cev, StreamEvent::Done) {
                    if self.error.is_some() {
                        continue;
                    }
                    self.done = true;
                }
                for chunk in self.in_tr.render_stream(&cev, &mut self.render) {
                    out.extend(chunk.to_bytes());
                }
            }
        }
        out
    }

    /// 流结束时若上游未显式发送 Done，补发终止分片
    pub fn finish(&mut self) -> Vec<u8> {
        if self.done {
            return Vec::new();
        }
        self.done = true;
        let mut out = Vec::new();
        for chunk in self
            .in_tr
            .render_stream(&StreamEvent::Done, &mut self.render)
        {
            out.extend(chunk.to_bytes());
        }
        out
    }
}
