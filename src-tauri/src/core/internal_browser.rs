use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
#[cfg(not(windows))]
use tauri::webview::PageLoadEvent;
use tauri::webview::{DownloadEvent, NewWindowFeatures, NewWindowResponse, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, Rect, Webview,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;
use url::Url;

#[cfg(windows)]
mod windows_navigation;

const CHROME_HEIGHT: f64 = 55.0;
static BROWSERS: LazyLock<Mutex<HashMap<String, BrowserState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static OPEN_LOCK: Mutex<()> = Mutex::new(());
static POPUP_TIMES: LazyLock<Mutex<VecDeque<Instant>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
const MAX_BROWSER_WINDOWS: usize = 8;
const MAX_POPUPS_PER_BURST: usize = 3;
const POPUP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    url: String,
    title: String,
    loading: bool,
    error: Option<String>,
    generation: u64,
    revision: u64,
    #[serde(skip)]
    navigation_id: Option<u64>,
    downloads: Vec<BrowserDownload>,
    chrome_height: f64,
    downloads_open: bool,
}

impl BrowserState {
    fn has_navigation_error(&self) -> bool {
        matches!(self.error.as_deref(), Some("loadFailed" | "loadTimeout"))
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub struct BrowserDownload {
    id: String,
    url: String,
    name: String,
    path: Option<String>,
    status: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Geometry {
    width: f64,
    height: f64,
    x: i32,
    y: i32,
    maximized: bool,
}

fn content_label(label: &str) -> String {
    label.to_string()
}
fn chrome_label(label: &str) -> String {
    format!("{label}_chrome")
}

fn update(
    app: &AppHandle,
    label: &str,
    change: impl FnOnce(&mut BrowserState),
) -> Option<BrowserState> {
    let snapshot = {
        let mut browsers = BROWSERS.lock().unwrap();
        let Some(state) = browsers.get_mut(label) else {
            return None;
        };
        let previous = state.clone();
        change(state);
        if *state == previous {
            return Some(previous);
        }
        state.revision += 1;
        state.clone()
    };
    let _ = app.emit_to(chrome_label(label), "internal-browser-state", &snapshot);
    Some(snapshot)
}

fn website_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "invalidUrl".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || matches!(url.host_str(), Some("tauri.localhost" | "ipc.localhost"))
    {
        return Err("invalidUrl".into());
    }
    Ok(url)
}

fn caller(webview: &Webview) -> Result<String, String> {
    let label = webview.window().label().to_string();
    if webview.label() != chrome_label(&label) || !BROWSERS.lock().unwrap().contains_key(&label) {
        return Err("Unknown browser window".into());
    }
    Ok(label)
}

fn resize_content(app: &AppHandle, window: &Window, chrome_height: f64) {
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let size = size.to_logical::<f64>(scale);
    let downloads_open = BROWSERS
        .lock()
        .unwrap()
        .get(window.label())
        .is_some_and(|state| state.downloads_open);
    if let Some(chrome) = app.get_webview(&chrome_label(window.label())) {
        let _ = chrome.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(
                size.width,
                if downloads_open {
                    size.height
                } else {
                    chrome_height
                },
            )
            .into(),
        });
    }
    if let Some(content) = app.get_webview(&content_label(window.label())) {
        let _ = content.set_bounds(Rect {
            position: LogicalPosition::new(0.0, chrome_height).into(),
            size: LogicalSize::new(size.width, (size.height - chrome_height).max(1.0)).into(),
        });
    }
}

fn geometry_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("internal-browser-window.json"))
}

fn begin_loading(app: &AppHandle, label: &str, url: &str, navigation_id: Option<u64>) {
    let Some(snapshot) = update(app, label, |state| {
        state.url = url.to_string();
        state.loading = true;
        state.error = None;
        state.generation += 1;
        state.navigation_id = navigation_id;
    }) else {
        return;
    };
    let generation = snapshot.generation;
    let app = app.clone();
    let label = label.to_string();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let _ = update(&app, &label, |state| {
            if state.generation == generation && state.loading {
                state.loading = false;
                state.error = Some("loadTimeout".into());
            }
        });
    });
}

#[cfg(windows)]
fn finish_navigation(app: &AppHandle, label: &str, navigation_id: u64, success: bool) {
    let _ = update(app, label, |state| {
        if state.navigation_id != Some(navigation_id) {
            return;
        }
        state.loading = false;
        state.error = if success {
            None
        } else {
            Some("loadFailed".into())
        };
        state.navigation_id = None;
    });
}

fn create_browser(app: &AppHandle, url: Url, title: Option<String>) -> Result<String, String> {
    let _guard = OPEN_LOCK.lock().map_err(|e| e.to_string())?;
    let existing = BROWSERS
        .lock()
        .unwrap()
        .iter()
        .find(|(_, state)| state.url == url.as_str())
        .map(|(label, _)| label.clone());
    if let Some(label) = existing {
        if let Some(window) = app.get_window(&label) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            return Ok(label);
        }
    }

    create_browser_window(app, url, title, None).map(|window| window.label().to_string())
}

fn create_browser_window(
    app: &AppHandle,
    url: Url,
    title: Option<String>,
    features: Option<NewWindowFeatures>,
) -> Result<WebviewWindow, String> {
    let is_popup = features.is_some();
    let label = format!("browser_{}", uuid::Uuid::new_v4().simple());
    let title = title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| url.host_str().unwrap_or("ATM").into());
    let geometry: Option<Geometry> = geometry_path(app)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let (width, height) = geometry
        .as_ref()
        .map(|g| (g.width.clamp(1.0, 3840.0), g.height.clamp(1.0, 2160.0)))
        .unwrap_or((1100.0, 760.0));
    {
        let mut browsers = BROWSERS.lock().unwrap();
        if browsers.len() >= MAX_BROWSER_WINDOWS {
            return Err("windowLimit".into());
        }
        if is_popup {
            let now = Instant::now();
            let mut times = POPUP_TIMES.lock().unwrap();
            while times
                .front()
                .is_some_and(|time| now.duration_since(*time) >= POPUP_INTERVAL)
            {
                times.pop_front();
            }
            if times.len() >= MAX_POPUPS_PER_BURST {
                return Err("popupThrottled".into());
            }
            times.push_back(now);
        }
        browsers.insert(
            label.clone(),
            BrowserState {
                url: url.to_string(),
                title: title.clone(),
                loading: url.as_str() != "about:blank",
                error: None,
                generation: 0,
                revision: 1,
                navigation_id: None,
                downloads: Vec::new(),
                chrome_height: CHROME_HEIGHT,
                downloads_open: false,
            },
        );
    }

    let result = (|| -> Result<WebviewWindow, String> {
        let navigation_app = app.clone();
        let navigation_label = label.clone();
        let popup_app = app.clone();
        let popup_label = label.clone();
        let download_app = app.clone();
        let download_label = label.clone();
        #[cfg(not(windows))]
        let load_app = app.clone();
        #[cfg(not(windows))]
        let load_label = label.clone();
        let title_app = app.clone();
        let title_label = label.clone();
        let initial_url = if cfg!(windows) {
            "about:blank".parse().unwrap()
        } else {
            url.clone()
        };
        let mut content = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(initial_url))
            .title(&title)
            .inner_size(width, height)
            .center()
            .visible(false)
            .incognito(false)
            .initialization_script(r#"
                document.addEventListener('keydown', event => {
                    if (event.isTrusted && !event.isComposing && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'l') {
                        event.preventDefault();
                        location.href = 'atm-browser-status://focus-address';
                    }
                }, true);
            "#)
            .on_navigation(move |target| {
                if target.scheme() == "atm-browser-status" {
                    if target.host_str() == Some("focus-address") {
                        if let Some(chrome) = navigation_app.get_webview(&chrome_label(&navigation_label)) {
                            let _ = chrome.set_focus();
                            let _ = navigation_app.emit_to(chrome_label(&navigation_label), "internal-browser-focus-address", ());
                        }
                        return false;
                    }
                    #[cfg(not(windows))]
                    {
                        let generation = target.query_pairs().find(|(key, _)| key == "generation").and_then(|(_, value)| value.parse::<u64>().ok());
                        let _ = update(&navigation_app, &navigation_label, |state| {
                            if Some(state.generation) == generation && target.host_str() == Some("failed") {
                                state.loading = false;
                                state.error = Some("loadFailed".into());
                            }
                        });
                    }
                    return false;
                }
                website_url(target.as_str()).is_ok() || target.as_str() == "about:blank" || target.scheme() == "blob"
            })
            .on_new_window(move |target, features| {
                if website_url(target.as_str()).is_ok() || target.as_str() == "about:blank" || target.scheme() == "blob" {
                    match create_browser_window(&popup_app, "about:blank".parse().unwrap(), Some(target.to_string()), Some(features)) {
                        Ok(window) => return NewWindowResponse::Create { window },
                        Err(error) => { let _ = update(&popup_app, &popup_label, |state| {
                            state.error = Some(if matches!(error.as_str(), "windowLimit" | "popupThrottled") { "popupBlocked" } else { "popupFailed" }.into());
                        }); },
                    }
                } else {
                    let _ = update(&popup_app, &popup_label, |state| state.error = Some("popupFailed".into()));
                }
                NewWindowResponse::Deny
            })
            .on_document_title_changed(move |webview, title| {
                let _ = webview.set_title(&title);
                let _ = update(&title_app, &title_label, |state| state.title = title);
            })
            .on_page_load(move |_webview, _payload| {
                #[cfg(not(windows))]
                {
                let webview = _webview;
                let payload = _payload;
                if payload.event() == PageLoadEvent::Started {
                    begin_loading(&load_app, &load_label, payload.url().as_str(), None);
                } else {
                    let _ = update(&load_app, &load_label, |state| {
                        if state.url != payload.url().as_str() { return }
                        if website_url(payload.url().as_str()).is_ok() { state.url = payload.url().to_string(); }
                        state.loading = false;
                        state.error = None;
                    });
                    let generation = BROWSERS.lock().unwrap().get(&load_label).map(|state| state.generation).unwrap_or(0);
                    let _ = webview.eval(format!(
                        "if (/^(chrome-error|edge-error):/.test(document.documentURI) || document.documentURI.startsWith('about:neterror')) {{ location.href = 'atm-browser-status://failed?generation={generation}'; }}"
                    ));
                }
                }
            })
            .on_download(move |_webview, event| {
                match event {
                    DownloadEvent::Requested { url, destination } => {
                        let id = uuid::Uuid::new_v4().simple().to_string();
                        let name = destination.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "download".into());
                        let Some(directory) = dirs::download_dir() else {
                            let _ = update(&download_app, &download_label, |state| state.error = Some("downloadFailed".into()));
                            return false;
                        };
                        let directory = directory.join("ATM").join(&id);
                        if std::fs::create_dir_all(&directory).is_err() {
                            let _ = update(&download_app, &download_label, |state| state.error = Some("downloadFailed".into()));
                            return false;
                        }
                        *destination = directory.join(&name);
                        let _ = update(&download_app, &download_label, |state| {
                            if state.url == url.as_str() {
                                state.loading = false;
                                state.error = None;
                                state.navigation_id = None;
                                state.generation += 1;
                            }
                            if state.downloads.len() >= 50 {
                                if let Some(index) = state.downloads.iter().position(|item| item.status != "downloading") { state.downloads.remove(index); }
                            }
                            state.downloads.push(BrowserDownload { id, url: url.to_string(), name,
                                path: Some(destination.to_string_lossy().into_owned()), status: "downloading".into() });
                        });
                    }
                    DownloadEvent::Finished { url, path, success } => {
                        let _ = update(&download_app, &download_label, |state| {
                            let index = path.as_ref().and_then(|path| state.downloads.iter().position(|item| item.path.as_deref() == path.to_str()))
                                .or_else(|| state.downloads.iter().position(|item| item.url == url.as_str() && item.status == "downloading"));
                            if let Some(index) = index {
                                let item = &mut state.downloads[index];
                                item.status = if success { "finished" } else { "failed" }.into();
                                if let Some(path) = path { item.path = Some(path.to_string_lossy().into_owned()); }
                            }
                        });
                    }
                    _ => (),
                }
                true
            });
        let profile = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("browser-profile");
        std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
        content = content.data_directory(profile);
        if let Ok(config) = crate::proxy_config::load_proxy_config_internal(app) {
            if config.enabled {
                if let Some(proxy) = config.build_proxy_url() {
                    let proxy = if let Some(rest) = proxy.strip_prefix("https://") {
                        format!("http://{rest}")
                    } else {
                        proxy
                    };
                    if let Ok(proxy) = Url::parse(&proxy) {
                        if matches!(proxy.scheme(), "http" | "socks5") {
                            content = content.proxy_url(proxy);
                        }
                    }
                }
            }
        }
        if let Some(features) = features {
            content = content.window_features(features);
        }
        content.build().map_err(|e| e.to_string())
    })();
    let root = match result {
        Ok(window) => window,
        Err(error) => {
            BROWSERS.lock().unwrap().remove(&label);
            return Err(error);
        }
    };
    let window = app.get_window(&label).ok_or("Browser window unavailable")?;
    #[cfg(windows)]
    if let Err(error) = windows_navigation::install(app, &label, &root) {
        BROWSERS.lock().unwrap().remove(&label);
        let _ = root.close();
        return Err(error);
    }
    let chrome = WebviewBuilder::new(
        chrome_label(&label),
        WebviewUrl::App("index.html?window=browser".into()),
    );
    if let Err(error) = window.add_child(
        chrome,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(width, CHROME_HEIGHT),
    ) {
        BROWSERS.lock().unwrap().remove(&label);
        let _ = root.close();
        return Err(error.to_string());
    }
    if let Some(g) = &geometry {
        let on_screen = window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .any(|monitor| {
                let area = monitor.work_area();
                g.x >= area.position.x
                    && g.y >= area.position.y
                    && g.x as i64 + 100 < area.position.x as i64 + area.size.width as i64
                    && g.y as i64 + 80 < area.position.y as i64 + area.size.height as i64
            });
        if on_screen {
            let _ = window.set_position(PhysicalPosition::new(g.x, g.y));
        }
    }

    let maximized = geometry.as_ref().is_some_and(|g| g.maximized);
    let position = window
        .outer_position()
        .unwrap_or(PhysicalPosition::new(0, 0));
    let saved = std::sync::Arc::new(Mutex::new(Geometry {
        width,
        height,
        x: position.x,
        y: position.y,
        maximized: false,
    }));
    let event_window = window.clone();
    let event_app = app.clone();
    let event_label = label.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Resized(_)
        | WindowEvent::Moved(_)
        | WindowEvent::ScaleFactorChanged { .. } => {
            let height = BROWSERS
                .lock()
                .unwrap()
                .get(&event_label)
                .map(|state| state.chrome_height)
                .unwrap_or(CHROME_HEIGHT);
            resize_content(&event_app, &event_window, height);
            if !event_window.is_maximized().unwrap_or(false)
                && !event_window.is_minimized().unwrap_or(false)
            {
                if let (Ok(size), Ok(position), Ok(scale)) = (
                    event_window.inner_size(),
                    event_window.outer_position(),
                    event_window.scale_factor(),
                ) {
                    let size = size.to_logical::<f64>(scale);
                    *saved.lock().unwrap() = Geometry {
                        width: size.width,
                        height: size.height,
                        x: position.x,
                        y: position.y,
                        maximized: false,
                    };
                }
            }
        }
        WindowEvent::CloseRequested { .. } => {
            let mut geometry = saved.lock().unwrap().clone();
            geometry.maximized = event_window.is_maximized().unwrap_or(false);
            if let Some(path) = geometry_path(&event_app) {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(json) = serde_json::to_vec(&geometry) {
                    let _ = std::fs::write(path, json);
                }
            }
        }
        WindowEvent::Destroyed => {
            BROWSERS.lock().unwrap().remove(&event_label);
        }
        _ => (),
    });
    if maximized {
        let _ = window.maximize();
    }
    resize_content(app, &window, CHROME_HEIGHT);
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    #[cfg(windows)]
    if !is_popup {
        if let Err(error) = root.navigate(url) {
            BROWSERS.lock().unwrap().remove(&label);
            let _ = root.close();
            return Err(error.to_string());
        }
    }
    Ok(root)
}

#[tauri::command]
pub async fn open_internal_browser(
    app: AppHandle,
    url: String,
    title: Option<String>,
) -> Result<String, String> {
    let url = website_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || create_browser(&app, url, title))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn internal_browser_state(
    app: AppHandle,
    webview: Webview,
) -> Result<BrowserState, String> {
    let label = caller(&webview)?;
    let revision = BROWSERS
        .lock()
        .unwrap()
        .get(&label)
        .map(|state| state.revision);
    if let Some(content) = app.get_webview(&content_label(&label)) {
        if let Ok(url) = content.url() {
            if website_url(url.as_str()).is_ok() {
                let mut browsers = BROWSERS.lock().unwrap();
                if let Some(state) = browsers.get_mut(&label).filter(|state| {
                    Some(state.revision) == revision
                        && !state.loading
                        && !state.has_navigation_error()
                        && state.url != url.as_str()
                }) {
                    state.url = url.to_string();
                    state.revision += 1;
                }
            }
        }
    }
    BROWSERS
        .lock()
        .unwrap()
        .get(&label)
        .cloned()
        .ok_or_else(|| "Unknown browser window".into())
}

#[tauri::command]
pub async fn internal_browser_action(
    app: AppHandle,
    webview: Webview,
    action: String,
    value: Option<String>,
) -> Result<(), String> {
    let label = caller(&webview)?;
    let content = app
        .get_webview(&content_label(&label))
        .ok_or("Browser not ready")?;
    match action.as_str() {
        "navigate" => {
            let url = website_url(value.as_deref().unwrap_or_default())?;
            // Fragment-only navigation has no document load; native events own loading state.
            content.navigate(url).map_err(|e| e.to_string())?;
        }
        "back" => {
            content.eval("history.back()").map_err(|e| e.to_string())?;
        }
        "forward" => {
            content
                .eval("history.forward()")
                .map_err(|e| e.to_string())?;
        }
        "reload" => {
            content.reload().map_err(|e| e.to_string())?;
        }
        "stop" => {
            content.eval("window.stop()").map_err(|e| e.to_string())?;
            let _ = update(&app, &label, |state| {
                state.loading = false;
                state.generation += 1;
                state.navigation_id = None;
            });
        }
        "copy" | "external" => {
            let pending = BROWSERS
                .lock()
                .unwrap()
                .get(&label)
                .filter(|state| state.loading || state.has_navigation_error())
                .map(|state| state.url.clone());
            let url = if let Some(url) = pending {
                website_url(&url)?
            } else {
                content.url().map_err(|e| e.to_string())?
            };
            let url = website_url(url.as_str()).or_else(|_| {
                let browsers = BROWSERS.lock().unwrap();
                website_url(&browsers.get(&label).ok_or("Unknown browser window")?.url)
            })?;
            if action == "copy" {
                arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(url.to_string()))
                    .map_err(|e| e.to_string())?;
            } else {
                app.opener()
                    .open_url(url.to_string(), None::<&str>)
                    .map_err(|e| e.to_string())?;
            }
        }
        "downloads" => {
            let open = value.as_deref() == Some("open");
            if open {
                content.hide().map_err(|e| e.to_string())?;
            } else {
                content.show().map_err(|e| e.to_string())?;
            }
            let _ = update(&app, &label, |state| state.downloads_open = open);
            let height = BROWSERS
                .lock()
                .unwrap()
                .get(&label)
                .map(|state| state.chrome_height)
                .unwrap_or(CHROME_HEIGHT);
            resize_content(&app, &webview.window(), height);
        }
        "layout" => {
            let height = value
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|height| height.is_finite())
                .unwrap_or(CHROME_HEIGHT)
                .clamp(52.0, 240.0);
            let _ = update(&app, &label, |state| state.chrome_height = height);
            resize_content(&app, &webview.window(), height);
        }
        "reveal" => {
            let path = BROWSERS
                .lock()
                .unwrap()
                .get(&label)
                .and_then(|state| {
                    state.downloads.iter().find(|item| {
                        Some(item.id.as_str()) == value.as_deref() && item.status == "finished"
                    })
                })
                .and_then(|item| item.path.clone())
                .ok_or("Download unavailable")?;
            app.opener()
                .reveal_item_in_dir(path)
                .map_err(|e| e.to_string())?;
        }
        _ => return Err("Unknown browser action".into()),
    }
    Ok(())
}
