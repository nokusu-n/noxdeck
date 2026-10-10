//! Our window's normal rectangle, saved only when it goes away. No webview permissions.
use crate::{
    lock,
    registry::{load, save, Loaded},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Default)]
pub struct Memory(Mutex<Option<Placement>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub normal: Rect,
    pub maximized: bool,
}

pub struct Monitor {
    pub work: Rect,
    pub scale: f64,
}

/// The primary monitor comes first. Physical screen pixels; defaults/minimum are logical.
pub fn choose(
    saved: Option<Placement>,
    monitors: &[Monitor],
    default: (f64, f64),
    minimum: (f64, f64),
) -> Option<Placement> {
    let primary = monitors.first()?;
    let target = saved.and_then(|p| {
        monitors.iter().find(|m| {
            let r = p.normal;
            // Keep a useful strip of the title bar reachable, including monitors left of primary.
            i64::from(r.x) + 160 <= i64::from(m.work.x) + i64::from(m.work.width)
                && i64::from(r.x) + i64::from(r.width) >= i64::from(m.work.x) + 160
                && r.y >= m.work.y
                && i64::from(r.y) + 40 <= i64::from(m.work.y) + i64::from(m.work.height)
        })
    });
    let monitor = target.unwrap_or(primary);
    let physical = |v: f64| (v * monitor.scale).round() as u32;
    let width = saved
        .map_or_else(|| physical(default.0), |p| p.normal.width)
        .max(physical(minimum.0))
        .min(monitor.work.width);
    let height = saved
        .map_or_else(|| physical(default.1), |p| p.normal.height)
        .max(physical(minimum.1))
        .min(monitor.work.height);
    let (x, y) = if let (Some(saved), Some(_)) = (saved, target) {
        (
            i64::from(saved.normal.x).clamp(
                i64::from(monitor.work.x),
                i64::from(monitor.work.x) + i64::from(monitor.work.width - width),
            ),
            i64::from(saved.normal.y).clamp(
                i64::from(monitor.work.y),
                i64::from(monitor.work.y) + i64::from(monitor.work.height - height),
            ),
        )
    } else {
        (
            i64::from(primary.work.x) + i64::from(primary.work.width - width) / 2,
            i64::from(primary.work.y) + i64::from(primary.work.height - height) / 2,
        )
    };
    Some(Placement {
        normal: Rect {
            x: x as i32,
            y: y as i32,
            width,
            height,
        },
        maximized: saved.is_some_and(|p| p.maximized),
    })
}

pub fn read(path: &Path) -> Option<Placement> {
    match load::<Placement>(path) {
        Loaded::Ok(p) if p.normal.width > 0 && p.normal.height > 0 => Some(p),
        _ => None,
    }
}

#[repr(C)]
#[derive(Default)]
struct NativeRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[link(name = "user32")]
extern "system" {
    fn GetWindowRect(window: *mut std::ffi::c_void, rect: *mut NativeRect) -> i32;
    fn GetClientRect(window: *mut std::ffi::c_void, rect: *mut NativeRect) -> i32;
    fn SetWindowPos(
        window: *mut std::ffi::c_void,
        after: *mut std::ffi::c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
}

/// Keep the normal rectangle in memory while Windows sends move/resize events.
/// Maximizing and minimizing never overwrite it; no file is written here.
pub fn track(window: &tauri::Window) -> tauri::Result<()> {
    use tauri::Manager;
    if window.is_minimized()? {
        return Ok(());
    }
    let maximized = window.is_maximized()?;
    let memory = window.state::<Memory>();
    if maximized {
        if let Some(p) = lock(&memory.0).as_mut() {
            p.maximized = true;
        }
    } else {
        let at = window.outer_position()?;
        let size = window.inner_size()?;
        *lock(&memory.0) = Some(Placement {
            normal: Rect {
                x: at.x,
                y: at.y,
                width: size.width,
                height: size.height,
            },
            maximized: false,
        });
    }
    Ok(())
}

pub fn remember(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    track(&window.as_ref().window())?;
    let Some(placement) = *lock(&app.state::<Memory>().0) else {
        return Ok(());
    };
    let core = app.state::<std::sync::Arc<crate::Core>>();
    // ponytail: disk-write failures keep the previous rectangle, like the other own data files;
    // add a storage error notice when read-only storage is supported.
    let _ = save(&core.paths.data.join("window.json"), &placement);
    Ok(())
}

fn set_bounds(window: &tauri::WebviewWindow, normal: Rect) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    let mut outer = NativeRect::default();
    let mut client = NativeRect::default();
    // Refresh the frameless non-client area before measuring it; the hidden caption
    // makes the framework's inner-size adjustment add 30 px on this Windows build.
    let ok = unsafe {
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, 0x0037) != 0
            && GetWindowRect(hwnd, &mut outer) != 0
            && GetClientRect(hwnd, &mut client) != 0
            && SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                normal.x,
                normal.y,
                normal.width as i32 + (outer.right - outer.left) - client.right,
                normal.height as i32 + (outer.bottom - outer.top) - client.bottom,
                0x0014,
            ) != 0
    };
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

pub fn restore(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use tauri::Manager;
    let primary = app.primary_monitor()?;
    let mut available = app.available_monitors()?;
    if let Some(primary) = primary {
        available.retain(|m| m.position() != primary.position());
        available.insert(0, primary);
    }
    let monitors: Vec<_> = available
        .iter()
        .map(|m| {
            let work = m.work_area();
            Monitor {
                work: Rect {
                    x: work.position.x,
                    y: work.position.y,
                    width: work.size.width,
                    height: work.size.height,
                },
                scale: m.scale_factor(),
            }
        })
        .collect();
    let core = app.state::<std::sync::Arc<crate::Core>>();
    let config = &app.config().app.windows[0];
    let saved = read(&core.paths.data.join("window.json"));
    if let Some(p) = choose(
        saved,
        &monitors,
        (config.width, config.height),
        (
            config.min_width.unwrap_or(0.0),
            config.min_height.unwrap_or(0.0),
        ),
    ) {
        window.set_position(tauri::PhysicalPosition::new(p.normal.x, p.normal.y))?;
        set_bounds(window, p.normal)?;
        *lock(&app.state::<Memory>().0) = Some(p);
        if p.maximized && config.visible {
            window.maximize()?;
        }
    }
    Ok(())
}
