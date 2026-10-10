//! Requests from the core to the desktop shell.
pub mod autostart;
pub mod instance;
pub mod window_state;
use crate::{tray, Core};
use std::sync::{mpsc, Arc};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellRequest {
    ShowWindow,
    HideWindow,
    Exit,
    TrayChanged,
}

pub trait Shell: Send + Sync {
    fn diagnostics_path(&self, _name: &str) -> std::io::Result<Option<std::path::PathBuf>> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
    fn request(&self, request: ShellRequest);
    fn open_url(&self, _url: &str) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
    fn autostart(&self) -> std::io::Result<bool> {
        Ok(false)
    }
    fn set_autostart(&self, _value: bool) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

impl<F: Fn(ShellRequest) + Send + Sync> Shell for F {
    fn request(&self, request: ShellRequest) {
        self(request);
    }
}

pub struct Desktop(pub mpsc::Sender<ShellRequest>);

impl Shell for Desktop {
    fn diagnostics_path(&self, name: &str) -> std::io::Result<Option<std::path::PathBuf>> {
        Ok(rfd::FileDialog::new().set_file_name(name).add_filter("TXT", &["txt"]).save_file())
    }
    fn open_url(&self, url: &str) -> std::io::Result<()> {
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(window: *mut std::ffi::c_void, verb: *const u16, file: *const u16, parameters: *const u16, directory: *const u16, show: i32) -> isize;
        }
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let result = unsafe { ShellExecuteW(std::ptr::null_mut(), wide("open").as_ptr(), wide(url).as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        if result > 32 { Ok(()) } else { Err(std::io::Error::other("repository opener failed")) }
    }
    fn request(&self, request: ShellRequest) {
        let _ = self.0.send(request);
    }
    fn autostart(&self) -> std::io::Result<bool> {
        Ok(autostart::Entry::configured().read()?.is_some())
    }
    fn set_autostart(&self, value: bool) -> std::io::Result<()> {
        autostart::Entry::configured().set(value, &std::env::current_exe()?)
    }
}

fn native_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for item in tray::menu(&app.state::<Arc<Core>>().state()) {
        // A BattleTag may contain '&'; native menus otherwise treat it as a mnemonic.
        let label = item.label.replace('&', "&&");
        menu.append(&MenuItem::with_id(
            app,
            item.id,
            label,
            item.enabled,
            None::<&str>,
        )?)?;
    }
    Ok(menu)
}

pub fn show_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let config = &app.config().app.windows[0];
    if let Some(window) = app.get_webview_window("main") {
        window.unminimize()?;
        if config.visible { window.show()?; }
        if config.visible && config.focus { window.set_focus()?; }
    } else {
        #[allow(unused_mut)]
        let mut builder =
            tauri::WebviewWindowBuilder::from_config(app, config)?.visible(false).focused(false);
        #[cfg(debug_assertions)]
        if crate::test_support::program_test() {
            builder = builder
                .initialization_script(crate::test_support::READY_SCRIPT)
                .on_document_title_changed(|window, title| {
                    if title == "Noxdeck [list ready]" {
                        let _ = window.set_title(&title);
                    }
                });
        }
        let window = builder.build()?;
        window_state::restore(app, &window)?;
        if config.visible { window.show()?; }
        if config.visible && config.focus { window.set_focus()?; }
    }
    app.state::<Arc<Core>>().journal.record(crate::diagnostics::Step::Window(crate::diagnostics::Window::AppShown));
    Ok(())
}

pub fn setup(
    app: &tauri::App,
    requests: mpsc::Receiver<ShellRequest>,
    start_in_tray: bool,
) -> tauri::Result<()> {
    app.manage(window_state::Memory::default());
    TrayIconBuilder::with_id("noxdeck")
        .icon(tauri::include_image!("icons/noxdeck-tray.ico"))
        .tooltip("Noxdeck")
        .menu(&native_menu(app.handle())?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let core = app.state::<Arc<Core>>().inner().clone();
            let id = event.id().as_ref().to_owned();
            // Launcher stopping is synchronous; keep it off the desktop event loop.
            std::thread::spawn(move || tray::click(&core, &id));
        })
        .on_tray_icon_event(|icon, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let core = icon.app_handle().state::<Arc<Core>>();
                core.shell.request(ShellRequest::ShowWindow);
            }
        })
        .build(app)?;
    if !start_in_tray {
        show_window(app.handle())?;
    }
    let app = app.handle().clone();
    std::thread::spawn(move || {
        for request in requests {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                let result = match request {
                    ShellRequest::ShowWindow => show_window(&handle),
                    ShellRequest::HideWindow => window_state::remember(&handle).and_then(|()| handle
                        .get_webview_window("main")
                        .map_or(Ok(()), |w| w.destroy())),
                    ShellRequest::Exit => {
                        let _ = window_state::remember(&handle);
                        handle.exit(0);
                        Ok(())
                    }
                    ShellRequest::TrayChanged => native_menu(&handle).and_then(|menu| {
                        handle
                            .tray_by_id("noxdeck")
                            .map_or(Ok(()), |icon| icon.set_menu(Some(menu)))
                    }),
                };
                assert!(result.is_ok(), "Noxdeck desktop request failed");
                if request == ShellRequest::HideWindow {
                    handle.state::<Arc<Core>>().journal.record(crate::diagnostics::Step::Window(crate::diagnostics::Window::AppDestroyed));
                }
            });
        }
    });
    Ok(())
}

pub fn window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if matches!(event, tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)) {
        let _ = window_state::track(window);
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        window
            .state::<Arc<Core>>()
            .shell
            .request(ShellRequest::HideWindow);
    }
}
