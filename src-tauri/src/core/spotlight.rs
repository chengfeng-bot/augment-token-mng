use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

static CURRENT_SHORTCUT: Mutex<Option<String>> = Mutex::new(None);
static FRONTEND_READY: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static VISIBILITY_EPOCH: AtomicU64 = AtomicU64::new(0);
static SHORTCUT_PRESSED: AtomicBool = AtomicBool::new(false);

const SPOTLIGHT_LABEL: &str = "spotlight";
const SPOTLIGHT_WIDTH: f64 = 680.0;
const SPOTLIGHT_HEIGHT: f64 = 480.0;

fn is_foreground(window: &tauri::Window) -> Result<bool, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        // Keyboard focus may belong to WebView2's child HWND while this window is active.
        Ok(unsafe { GetForegroundWindow().0 == hwnd.0 })
    }
    #[cfg(not(windows))]
    {
        window.is_focused().map_err(|e| e.to_string())
    }
}

fn present_spotlight(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SPOTLIGHT_LABEL)
        .ok_or("Spotlight window missing")?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    if let Some(webview) = app.get_webview(SPOTLIGHT_LABEL) {
        webview.set_focus().map_err(|e| e.to_string())?;
    }
    let _ = app.emit_to(SPOTLIGHT_LABEL, "spotlight-input-focus", ());
    Ok(())
}

/// 创建或显示 Spotlight 搜索窗口
fn show_spotlight_window(app: &AppHandle) -> Result<(), String> {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
    VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst);
    if app.get_webview_window(SPOTLIGHT_LABEL).is_some() {
        if FRONTEND_READY.load(Ordering::SeqCst) {
            present_spotlight(app)?;
            let _ = app.emit_to(SPOTLIGHT_LABEL, "spotlight-focus", ());
        }
        return Ok(());
    }

    FRONTEND_READY.store(false, Ordering::SeqCst);
    let url = WebviewUrl::App("index.html?window=spotlight".into());

    let window = WebviewWindowBuilder::new(app, SPOTLIGHT_LABEL, url)
        .title("Quick Search")
        .inner_size(SPOTLIGHT_WIDTH, SPOTLIGHT_HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .center()
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| {
            SHOW_REQUESTED.store(false, Ordering::SeqCst);
            format!("Failed to create spotlight window: {}", e)
        })?;

    // 窗口失焦时自动隐藏
    let app_handle = app.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Focused(true) => {
            let _ = app_handle.emit_to(SPOTLIGHT_LABEL, "spotlight-input-focus", ());
        }
        tauri::WindowEvent::Focused(false) => {
            if !FRONTEND_READY.load(Ordering::SeqCst) || !SHOW_REQUESTED.load(Ordering::SeqCst) {
                return;
            }
            let epoch = VISIBILITY_EPOCH.load(Ordering::SeqCst);
            let app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if epoch != VISIBILITY_EPOCH.load(Ordering::SeqCst)
                        || !SHOW_REQUESTED.load(Ordering::SeqCst)
                    {
                        return;
                    }
                    if let Some(window) = handle.get_window(SPOTLIGHT_LABEL) {
                        if window.is_visible().unwrap_or(false)
                            && matches!(is_foreground(&window), Ok(false))
                        {
                            let _ = hide_spotlight(handle);
                        }
                    }
                });
            });
        }
        tauri::WindowEvent::Destroyed => {
            FRONTEND_READY.store(false, Ordering::SeqCst);
            SHOW_REQUESTED.store(false, Ordering::SeqCst);
            VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst);
        }
        _ => (),
    });

    Ok(())
}

#[tauri::command]
pub fn spotlight_ready(app: AppHandle, webview: tauri::Webview) -> Result<bool, String> {
    if webview.label() != SPOTLIGHT_LABEL || webview.window().label() != SPOTLIGHT_LABEL {
        return Err("Not a Spotlight window".into());
    }
    FRONTEND_READY.store(true, Ordering::SeqCst);
    if !SHOW_REQUESTED.load(Ordering::SeqCst) {
        return Ok(false);
    }
    present_spotlight(&app)?;
    Ok(true)
}

/// 隐藏 Spotlight 搜索窗口
#[tauri::command]
pub fn hide_spotlight(app: AppHandle) -> Result<(), String> {
    SHOW_REQUESTED.store(false, Ordering::SeqCst);
    VISIBILITY_EPOCH.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window(SPOTLIGHT_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 切换 Spotlight 窗口显示状态
#[tauri::command]
pub fn toggle_spotlight(app: AppHandle) -> Result<(), String> {
    if SHOW_REQUESTED.load(Ordering::SeqCst) {
        hide_spotlight(app)
    } else {
        show_spotlight_window(&app)
    }
}

/// 注册全局快捷键
#[tauri::command]
pub fn register_spotlight_shortcut(app: AppHandle, shortcut: String) -> Result<(), String> {
    let shortcut_parsed: Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut '{}': {}", shortcut, e))?;

    let gsm = app.global_shortcut();

    // 先取消旧的 spotlight 快捷键（不影响其他全局快捷键）
    if let Ok(mut current) = CURRENT_SHORTCUT.lock() {
        if let Some(old_str) = current.take() {
            if let Ok(old_shortcut) = old_str.parse::<Shortcut>() {
                let _ = gsm.unregister(old_shortcut);
            }
        }
    }

    let app_clone = app.clone();
    SHORTCUT_PRESSED.store(false, Ordering::SeqCst);
    gsm.on_shortcut(shortcut_parsed, move |_app, _shortcut, event| {
        match event.state {
            ShortcutState::Pressed if !SHORTCUT_PRESSED.swap(true, Ordering::SeqCst) => {
                if let Err(error) = toggle_spotlight(app_clone.clone()) {
                    eprintln!("Failed to toggle spotlight: {error}");
                }
            }
            ShortcutState::Released => {
                SHORTCUT_PRESSED.store(false, Ordering::SeqCst);
            }
            _ => (),
        }
    })
    .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // 记录当前注册的快捷键
    if let Ok(mut current) = CURRENT_SHORTCUT.lock() {
        *current = Some(shortcut);
    }

    Ok(())
}

/// 取消注册全局快捷键
#[tauri::command]
pub fn unregister_spotlight_shortcut(app: AppHandle) -> Result<(), String> {
    SHORTCUT_PRESSED.store(false, Ordering::SeqCst);
    let gsm = app.global_shortcut();

    if let Ok(mut current) = CURRENT_SHORTCUT.lock() {
        if let Some(old_str) = current.take() {
            if let Ok(old_shortcut) = old_str.parse::<Shortcut>() {
                gsm.unregister(old_shortcut)
                    .map_err(|e| format!("Failed to unregister shortcut: {}", e))?;
            }
        }
    }

    Ok(())
}

/// 获取当前快捷键注册状态
#[tauri::command]
pub fn is_spotlight_shortcut_registered(app: AppHandle, shortcut: String) -> Result<bool, String> {
    let shortcut_parsed: Shortcut = shortcut
        .parse()
        .map_err(|e| format!("Invalid shortcut '{}': {}", shortcut, e))?;

    Ok(app.global_shortcut().is_registered(shortcut_parsed))
}
