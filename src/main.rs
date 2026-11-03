#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, DragDropEvent, Emitter, Manager, State, WebviewWindow, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;

const MAIN_WINDOW: &str = "main";
const EVENT_OPEN_DOCUMENT: &str = "open-document";
#[cfg(target_os = "macos")]
const EVENT_MENU_ACTION: &str = "menu-action";

/// Documents that arrived before the webview registered its listener.
#[derive(Default)]
struct Inbox {
    ready: AtomicBool,
    queued: Mutex<Vec<serde_json::Value>>,
}

/// The file the editor is working on, if any. Paths stay on this side; the
/// webview only ever sees file names.
#[derive(Default)]
struct Document {
    path: Mutex<Option<PathBuf>>,
}

const LAST_DOCUMENT_FILE: &str = "last-document";

fn main() {
    linux_webkit_workarounds();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            present(app);
            open_paths(app, args.into_iter().skip(1).map(PathBuf::from));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Inbox::default())
        .manage(Document::default())
        .invoke_handler(tauri::generate_handler![
            pending_documents,
            open_document_dialog,
            save_document,
            save_document_as,
            close_document,
            shared_rates,
            store_shared_rates,
            hide_window
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                install_menu(app.handle())?;
            }
            install_tray(app.handle())?;
            let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
            if args.is_empty() {
                open_paths(app.handle(), last_document(app.handle()));
            } else {
                open_paths(app.handle(), args);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window hides it; the tray or a second launch brings it back.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                open_paths(window.app_handle(), paths.iter().cloned());
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("failed to build numr")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            match event {
                tauri::RunEvent::Opened { urls } => {
                    open_paths(app, urls.iter().filter_map(|url| url.to_file_path().ok()));
                }
                tauri::RunEvent::Reopen { .. } => show(app),
                _ => {}
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Called once by the webview after it registered its `open-document` listener.
#[tauri::command]
fn pending_documents(inbox: State<'_, Inbox>) -> Vec<serde_json::Value> {
    let mut queued = inbox.queued.lock().expect("inbox poisoned");
    inbox.ready.store(true, Ordering::SeqCst);
    std::mem::take(&mut *queued)
}

/// Async so the blocking dialog runs off the main thread.
#[tauri::command]
async fn open_document_dialog(app: AppHandle) {
    let picked = app
        .dialog()
        .file()
        .add_filter("numr document", &["numr", "txt"])
        .blocking_pick_file();
    open_paths(&app, picked.and_then(|p| p.into_path().ok()));
}

/// Writes to the open file, or asks for one first. Returns the file name, or
/// `None` when the user cancelled.
#[tauri::command]
async fn save_document(app: AppHandle, content: String) -> Result<Option<String>, String> {
    let current = app
        .state::<Document>()
        .path
        .lock()
        .expect("document poisoned")
        .clone();
    match current {
        Some(path) => write_document(&app, &path, &content).map(Some),
        None => save_document_as(app, content).await,
    }
}

#[tauri::command]
async fn save_document_as(app: AppHandle, content: String) -> Result<Option<String>, String> {
    let suggested = app
        .state::<Document>()
        .path
        .lock()
        .expect("document poisoned")
        .as_deref()
        .and_then(file_name)
        .unwrap_or_else(|| "calculations.numr".to_string());
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("numr document", &["numr"])
        .set_file_name(&suggested)
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    write_document(&app, &path, &content).map(Some)
}

/// The editor started a new, unsaved document.
#[tauri::command]
fn close_document(app: AppHandle) {
    set_current_path(&app, None);
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

// ---------------------------------------------------------------------------
// Exchange-rate cache shared with the numr TUI and CLI
// ---------------------------------------------------------------------------

/// numr-core writes `{"timestamp": <unix seconds>, "rates": {"EUR": 0.92, …}}`
/// to `<config>/numr/rates.json` and treats entries older than an hour as expired.
const SHARED_RATES_TTL_SECS: u64 = 3600;

fn shared_rates_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "numr").map(|dirs| dirs.config_dir().join("rates.json"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Fresh shared rates as `{ rates, timestamp }` with a millisecond timestamp, or `None`.
#[tauri::command]
fn shared_rates() -> Option<serde_json::Value> {
    let content = fs::read_to_string(shared_rates_path()?).ok()?;
    let cached: serde_json::Value = serde_json::from_str(&content).ok()?;
    let timestamp = cached.get("timestamp")?.as_u64()?;
    let now = unix_now();
    if timestamp > now || now - timestamp >= SHARED_RATES_TTL_SECS {
        return None;
    }
    let rates = cached.get("rates")?.as_object()?;
    (!rates.is_empty())
        .then(|| serde_json::json!({ "rates": rates, "timestamp": timestamp * 1000 }))
}

/// Writes rates fetched by the webview in numr-core's format, atomically.
#[tauri::command]
fn store_shared_rates(
    rates: serde_json::Map<String, serde_json::Value>,
    timestamp: u64,
) -> Result<(), String> {
    let path = shared_rates_path().ok_or("no config directory")?;
    let content = serde_json::to_string_pretty(&serde_json::json!({
        "timestamp": timestamp / 1000,
        "rates": rates,
    }))
    .map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("no parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".rates.json.{}", std::process::id()));
    fs::write(&temp, content)
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|e| {
            let _ = fs::remove_file(&temp);
            format!("{}: {e}", path.display())
        })
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn pick_and_open(app: &AppHandle) {
    let handle = app.clone();
    app.dialog()
        .file()
        .add_filter("numr document", &["numr", "txt"])
        .pick_file(move |picked| {
            open_paths(&handle, picked.and_then(|p| p.into_path().ok()));
        });
}

fn open_paths(app: &AppHandle, paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        match fs::read_to_string(&path) {
            Ok(content) => {
                set_current_path(app, Some(path.clone()));
                deliver(app, content, file_name(&path));
            }
            Err(error) => eprintln!("numr: failed to open {}: {error}", path.display()),
        }
    }
}

fn write_document(app: &AppHandle, path: &Path, content: &str) -> Result<String, String> {
    fs::write(path, content).map_err(|error| format!("{}: {error}", path.display()))?;
    set_current_path(app, Some(path.to_path_buf()));
    Ok(file_name(path).unwrap_or_default())
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Remembers the open file in app state and on disk, so the next launch
/// without arguments reopens it.
fn set_current_path(app: &AppHandle, path: Option<PathBuf>) {
    *app.state::<Document>()
        .path
        .lock()
        .expect("document poisoned") = path.clone();
    let Some(marker) = last_document_marker(app) else {
        return;
    };
    let result = match path {
        Some(path) => fs::create_dir_all(marker.parent().unwrap_or(Path::new(".")))
            .and_then(|_| fs::write(&marker, path.to_string_lossy().as_bytes())),
        None => match fs::remove_file(&marker) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
    };
    if let Err(error) = result {
        eprintln!("numr: failed to remember the open document: {error}");
    }
}

fn last_document_marker(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(LAST_DOCUMENT_FILE))
}

fn last_document(app: &AppHandle) -> Option<PathBuf> {
    let marker = last_document_marker(app)?;
    let path = PathBuf::from(fs::read_to_string(marker).ok()?.trim());
    path.is_file().then_some(path)
}

fn deliver(app: &AppHandle, content: String, name: Option<String>) {
    let payload = serde_json::json!({ "content": content, "name": name });
    let inbox = app.state::<Inbox>();
    let mut queued = inbox.queued.lock().expect("inbox poisoned");
    if inbox.ready.load(Ordering::SeqCst) {
        let _ = app.emit_to(MAIN_WINDOW, EVENT_OPEN_DOCUMENT, payload);
    } else {
        queued.push(payload);
    }
    show(app);
}

// ---------------------------------------------------------------------------
// Window, tray, menu
// ---------------------------------------------------------------------------

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

fn show(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Bring the window to the user even when it lives on another workspace.
/// Wayland compositors ignore focus requests without an activation token (the
/// token belongs to the second process, which only forwards its arguments), but
/// remapping the surface makes them place it on the current workspace.
fn present(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        }
    }
    show(app);
}

fn toggle(app: &AppHandle) {
    match main_window(app) {
        Some(window) if window.is_visible().unwrap_or(false) => {
            let _ = window.hide();
        }
        _ => show(app),
    }
}

fn install_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_item = MenuItemBuilder::with_id("toggle", "Show / Hide numr").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit numr").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&toggle_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let icon_bytes: &[u8] = if cfg!(target_os = "macos") {
        include_bytes!("../icons/tray/tray-template.png")
    } else {
        include_bytes!("../icons/tray/tray.png")
    };

    TrayIconBuilder::with_id("numr")
        .icon(tauri::image::Image::from_bytes(icon_bytes)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("numr")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => toggle(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Native menu bar. macOS only: Linux would draw it inside the frameless
/// window, and keyboard shortcuts there live in the webview bridge instead.
#[cfg(target_os = "macos")]
fn install_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::SubmenuBuilder;

    let item = |id: &str, label: &str, accelerator: &str| {
        let item = MenuItemBuilder::with_id(id, label);
        if accelerator.is_empty() {
            item.build(app)
        } else {
            item.accelerator(accelerator).build(app)
        }
    };

    let app_menu = SubmenuBuilder::new(app, "numr")
        .about(None)
        .separator()
        .hide()
        .hide_others()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item("new-document", "New", "CmdOrCtrl+N")?)
        .item(&item("open-document", "Open…", "CmdOrCtrl+O")?)
        .item(&item("save-document", "Save", "CmdOrCtrl+S")?)
        .item(&item("save-document-as", "Save As…", "CmdOrCtrl+Shift+S")?)
        .separator()
        .item(&item("publish-document", "Publish", "")?)
        .separator()
        .item(&item("hide-window", "Hide numr", "CmdOrCtrl+W")?)
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item("toggle-line-wrapping", "Wrap Lines", "Alt+Z")?)
        .separator()
        .item(&item("refresh-rates", "Refresh Rates", "CmdOrCtrl+R")?)
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view])
        .build()?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        "open-document" => pick_and_open(app),
        "hide-window" => {
            if let Some(window) = main_window(app) {
                let _ = window.hide();
            }
        }
        "toggle" | "quit" => {}
        action => {
            let _ = app.emit_to(MAIN_WINDOW, EVENT_MENU_ACTION, action);
        }
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Platform quirks
// ---------------------------------------------------------------------------

fn linux_webkit_workarounds() {
    #[cfg(target_os = "linux")]
    {
        // WebKitGTK on the proprietary NVIDIA driver renders a blank window
        // unless DMA-BUF is disabled. Users can still override the variable.
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
            && std::path::Path::new("/proc/driver/nvidia/version").exists()
        {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }
}
