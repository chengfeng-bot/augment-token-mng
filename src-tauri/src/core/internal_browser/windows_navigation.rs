use std::{sync::mpsc, time::Duration};
use tauri::{AppHandle, WebviewWindow};
use webview2_com::{NavigationCompletedEventHandler, NavigationStartingEventHandler, take_pwstr};

pub(super) fn install(app: &AppHandle, label: &str, window: &WebviewWindow) -> Result<(), String> {
    let app = app.clone();
    let label = label.to_string();
    let (tx, rx) = mpsc::channel();
    window
        .with_webview(move |platform| {
            let result = (|| -> Result<(), String> {
                // Tauri invokes this closure on the WebView2 UI thread.
                unsafe {
                    let webview = platform
                        .controller()
                        .CoreWebView2()
                        .map_err(|e| e.to_string())?;
                    let start_app = app.clone();
                    let start_label = label.clone();
                    let starting =
                        NavigationStartingEventHandler::create(Box::new(move |_, args| {
                            let Some(args) = args else { return Ok(()) };
                            let mut cancelled = Default::default();
                            args.Cancel(&mut cancelled)?;
                            if cancelled.as_bool() {
                                return Ok(());
                            }
                            let mut uri = Default::default();
                            args.Uri(&mut uri)?;
                            let uri = take_pwstr(uri);
                            let Ok(url) = url::Url::parse(&uri) else {
                                return Ok(());
                            };
                            if super::website_url(&uri).is_err() && url.scheme() != "blob" {
                                return Ok(());
                            }
                            let mut navigation_id = 0;
                            args.NavigationId(&mut navigation_id)?;
                            super::begin_loading(
                                &start_app,
                                &start_label,
                                &uri,
                                Some(navigation_id),
                            );
                            Ok(())
                        }));
                    let completed =
                        NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                            let Some(args) = args else { return Ok(()) };
                            let mut navigation_id = 0;
                            let mut success = Default::default();
                            args.NavigationId(&mut navigation_id)?;
                            args.IsSuccess(&mut success)?;
                            super::finish_navigation(
                                &app,
                                &label,
                                navigation_id,
                                success.as_bool(),
                            );
                            Ok(())
                        }));
                    let mut starting_token = 0;
                    let mut completed_token = 0;
                    webview
                        .add_NavigationStarting(&starting, &mut starting_token)
                        .map_err(|e| e.to_string())?;
                    if let Err(error) =
                        webview.add_NavigationCompleted(&completed, &mut completed_token)
                    {
                        let _ = webview.remove_NavigationStarting(starting_token);
                        return Err(error.to_string());
                    }
                }
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    // with_webview runs inline for UI-thread popups and queues work for background opens.
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|e| e.to_string())?
}
