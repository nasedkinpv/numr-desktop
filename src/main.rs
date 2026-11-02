#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs,
    path::PathBuf,
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
    queued: Mutex<Vec<String>>,
}

fn main() {
    linux_webkit_workarounds();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show(app);
            open_paths(app, args.into_iter().skip(1).map(PathBuf::from));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Inbox::default())
        .invoke_handler(tauri::generate_handler![
            pending_documents,
            open_document_dialog,
            save_document,
            hide_window
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                install_menu(app.handle())?;
            }
            install_tray(app.handle())?;
            open_paths(app.handle(), std::env::args_os().skip(1).map(PathBuf::from));
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
fn pending_documents(inbox: State<'_, Inbox>) -> Vec<String> {
    let mut queued = inbox.queued.lock().expect("inbox poisoned");
    inbox.ready.store(true, Ordering::SeqCst);
    std::mem::take(&mut *queued)
}

#[tauri::command]
fn open_document_dialog(app: AppHandle) {
    pick_and_open(&app);
}

#[tauri::command]
fn save_document(app: AppHandle, content: String) {
    app.dialog()
        .file()
        .add_filter("numr document", &["numr"])
        .set_file_name("calculations.numr")
        .save_file(move |picked| {
            let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
                return;
            };
            if let Err(error) = fs::write(&path, content) {
                eprintln!("numr: failed to save {}: {error}", path.display());
            }
        });
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

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
            Ok(content) => deliver(app, content),
            Err(error) => eprintln!("numr: failed to open {}: {error}", path.display()),
        }
    }
}

fn deliver(app: &AppHandle, content: String) {
    let inbox = app.state::<Inbox>();
    let mut queued = inbox.queued.lock().expect("inbox poisoned");
    if inbox.ready.load(Ordering::SeqCst) {
        let _ = app.emit_to(MAIN_WINDOW, EVENT_OPEN_DOCUMENT, content);
    } else {
        queued.push(content);
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
