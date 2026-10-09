//! The real `Processes`: matches launcher processes by the directory their
//! executable lives in, never by name, so a stand-in install cannot reach the
//! real launcher.

use super::{
    ProcessError, ProcessSnapshot, Processes, CLIENT_EXE, GAME_LAUNCH_COMMAND, LAUNCHER_EXE,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Default)]
pub struct RealProcesses {
    hidden: Mutex<Vec<(usize, u32)>>,
    launcher_ids: Mutex<(PathBuf, Vec<u32>)>,
    refreshing: Mutex<()>,
    #[cfg(test)]
    snapshot_count: std::sync::atomic::AtomicUsize,
}

impl RealProcesses {
    fn restore_windows(&self, install: &Path, command: i32) {
        let allowed = self.launcher_pids(install);
        let mut hidden = self.hidden.lock().unwrap_or_else(|e| e.into_inner());
        for (handle, remembered) in std::mem::take(&mut *hidden) {
            let window = handle as Hwnd;
            if unsafe { IsWindow(window) != 0 }
                && scoped_owner(owner(window), remembered, &allowed)
                && scoped_executable(remembered, install, executable)
            {
                unsafe {
                    ShowWindowAsync(window, command);
                }
            }
        }
    }


    fn window_pids(&self, install: &Path) -> Vec<u32> {
        let cached = self.launcher_ids.lock().unwrap_or_else(|e| e.into_inner());
        if norm(&cached.0) == norm(install) {
            cached.1.clone()
        } else {
            vec![]
        }
    }
    fn launcher_pids(&self, install: &Path) -> Vec<u32> {
        #[cfg(test)]
        self.snapshot_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        launcher_pids(install)
    }
}

fn game_command(install: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(install.join(CLIENT_EXE));
    command
        .arg(GAME_LAUNCH_COMMAND)
        .current_dir(install)
        .creation_flags(0x0800_0000);
    command
}

fn scoped_owner(current: u32, remembered: u32, allowed: &[u32]) -> bool {
    current != 0 && current == remembered && allowed.contains(&current)
}

fn scoped_executable(pid: u32, install: &Path, exe: impl FnOnce(u32) -> Option<PathBuf>) -> bool {
    let dir = format!("{}\\", norm(install).trim_end_matches('\\'));
    exe(pid).is_some_and(|path| norm(&path).starts_with(&dir))
}

type Hwnd = *mut std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Hwnd;
    fn QueryFullProcessImageNameW(process: Hwnd, flags: u32, name: *mut u16, size: *mut u32)
        -> i32;
    fn CloseHandle(handle: Hwnd) -> i32;
}

fn executable(pid: u32) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    unsafe {
        let process = OpenProcess(0x1000, 0, pid); // PROCESS_QUERY_LIMITED_INFORMATION
        if process.is_null() {
            return None;
        }
        let mut name = vec![0u16; 32768];
        let mut len = name.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        ok.then(|| std::ffi::OsString::from_wide(&name[..len as usize]).into())
    }
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, isize) -> i32, data: isize) -> i32;
    fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
    fn IsWindowVisible(window: Hwnd) -> i32;
    fn IsWindow(window: Hwnd) -> i32;
    fn ShowWindowAsync(window: Hwnd, command: i32) -> i32;
}

unsafe extern "system" fn collect_window(window: Hwnd, data: isize) -> i32 {
    // EnumWindows calls synchronously while the supplied vector is alive.
    (*(data as *mut Vec<usize>)).push(window as usize);
    1
}

fn owner(window: Hwnd) -> u32 {
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
    }
    pid
}

fn launcher_pids(install: &Path) -> Vec<u32> {
    inside(&snapshot(), install)
        .iter()
        .map(|p| p.pid().as_u32())
        .collect()
}

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    s.strip_prefix("\\\\?\\").map(str::to_owned).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_passes_do_not_take_process_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let processes = RealProcesses::default();
        *processes.launcher_ids.lock().unwrap() = (dir.path().to_owned(), vec![]);
        for _ in 0..3 {
            processes.hide_windows(dir.path());
        }
        assert_eq!(
            processes
                .snapshot_count
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn cached_window_pids_cannot_be_used_for_another_install_directory() {
        let dir = tempfile::tempdir().unwrap();
        let processes = RealProcesses::default();
        *processes.launcher_ids.lock().unwrap() = (dir.path().to_owned(), vec![101, 102]);
        assert_eq!(processes.window_pids(dir.path()), vec![101, 102]);
        assert!(processes.window_pids(&dir.path().join("other")).is_empty());
        assert_eq!(
            processes
                .snapshot_count
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[test]
    fn recycled_process_ids_are_checked_again_before_acting_on_a_window() {
        let dir = Path::new("C:\\StandIn\\Install");
        assert!(scoped_executable(101, dir, |_| Some(
            dir.join("Launcher.exe")
        )));
        assert!(!scoped_executable(101, dir, |_| Some(
            Path::new("C:\\PrivateLauncher\\Launcher.exe").into()
        )));
        assert!(!scoped_executable(101, dir, |_| Some(
            Path::new("C:\\StandIn\\InstallOther\\Launcher.exe").into()
        )));
        assert!(!scoped_executable(101, dir, |_| None));
    }

    #[test]
    fn launch_command_is_one_argument_to_the_given_install() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = game_command(dir.path());
        assert_eq!(cmd.get_program(), dir.path().join(super::super::CLIENT_EXE));
        assert_eq!(
            cmd.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("--exec=launch Pro")]
        );
        assert_eq!(cmd.get_current_dir(), Some(dir.path()));
    }

    #[test]
    fn windows_require_a_current_owner_inside_the_given_install() {
        assert!(scoped_owner(100, 100, &[100, 101]));
        assert!(!scoped_owner(200, 200, &[100, 101]), "outside the install");
        assert!(!scoped_owner(0, 0, &[100, 101]), "destroyed window");
        assert!(!scoped_owner(101, 100, &[100, 101]), "handle changed owner");
        assert!(!scoped_owner(100, 100, &[]), "launcher no longer exists");
    }
}

fn snapshot() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    sys
}

fn inside<'a>(sys: &'a System, install: &Path) -> Vec<&'a sysinfo::Process> {
    let dir = format!("{}\\", norm(install).trim_end_matches('\\'));
    sys.processes()
        .values()
        .filter(|p| p.exe().map(|e| norm(e).starts_with(&dir)).unwrap_or(false))
        .collect()
}

fn named<'a>(sys: &'a System, exe_name: &str) -> Vec<&'a sysinfo::Process> {
    sys.processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe_name))
        .collect()
}

/// Kill every given process; a refusal from the system is "access denied".
fn kill_all(procs: Vec<&sysinfo::Process>) -> Result<(), ProcessError> {
    let mut denied = false;
    for p in procs {
        if !p.kill() {
            denied = true;
        }
    }
    if denied {
        Err(ProcessError::AccessDenied)
    } else {
        Ok(())
    }
}

impl Processes for RealProcesses {
    fn refresh_launcher_pids(&self, install: &Path) {
        let _refresh = self.refreshing.lock().unwrap_or_else(|e| e.into_inner());
        let ids = self.launcher_pids(install);
        *self.launcher_ids.lock().unwrap_or_else(|e| e.into_inner()) = (install.to_owned(), ids);
    }
    fn send_launch_command(&self, install: &Path) -> Result<(), ProcessError> {
        game_command(install)
            .spawn()
            .map(|_| ())
            .map_err(|e| ProcessError::Other(e.to_string()))
    }
    fn hide_windows(&self, install: &Path) {
        let allowed = self.window_pids(install);
        if allowed.is_empty() {
            return;
        }
        let mut windows = Vec::<usize>::new();
        unsafe {
            EnumWindows(collect_window, &mut windows as *mut _ as isize);
        }
        let mut hidden = self.hidden.lock().unwrap_or_else(|e| e.into_inner());
        hidden.retain(|(handle, remembered)| {
            scoped_owner(owner(*handle as Hwnd), *remembered, &allowed)
        });
        for handle in windows {
            let window = handle as Hwnd;
            let pid = owner(window);
            if scoped_owner(pid, pid, &allowed)
                && unsafe { IsWindowVisible(window) != 0 }
                && scoped_executable(pid, install, executable)
            {
                if !hidden.contains(&(handle, pid)) {
                    hidden.push((handle, pid));
                }
                // Post the change; a hung foreign UI must not block the bounded operation.
                unsafe {
                    ShowWindowAsync(window, 0);
                }
            }
        }
    }
    fn show_windows(&self, install: &Path) {
        self.restore_windows(install, 5); // SW_SHOW
    }
    fn show_windows_no_activate(&self, install: &Path) {
        self.restore_windows(install, 4); // SW_SHOWNOACTIVATE: restore without activation
    }

    fn snapshot(&self, install: Option<&Path>, exe_name: &str) -> ProcessSnapshot {
        let sys = snapshot();
        let mut game_pids: Vec<_> = named(&sys, exe_name)
            .iter()
            .map(|p| p.pid().as_u32())
            .collect();
        game_pids.sort_unstable();
        ProcessSnapshot {
            launcher_running: install.is_some_and(|dir| !inside(&sys, dir).is_empty()),
            game_running: !game_pids.is_empty(),
            game_pids,
        }
    }

    fn launcher_running(&self, install: &Path) -> bool {
        !inside(&snapshot(), install).is_empty()
    }

    fn stop_launcher(&self, install: &Path, timeout: Duration) -> Result<(), ProcessError> {
        let deadline = Instant::now() + timeout;
        loop {
            let sys = snapshot();
            let procs = inside(&sys, install);
            if procs.is_empty() {
                return Ok(());
            }
            if let Err(e) = kill_all(procs) {
                // A process that vanished meanwhile makes `kill` fail too: look again.
                if !inside(&snapshot(), install).is_empty() {
                    return Err(e);
                }
            }
            if Instant::now() >= deadline {
                return Err(ProcessError::Other(
                    "the launcher did not stop in time".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn start_launcher(&self, install: &Path) -> Result<(), ProcessError> {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(install.join(LAUNCHER_EXE))
            .current_dir(install)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: no console for a console stand-in
            .spawn()
            .map(|_| ())
            .map_err(|e| ProcessError::Other(e.to_string()))
    }

    fn game_running(&self, exe_name: &str) -> bool {
        !named(&snapshot(), exe_name).is_empty()
    }

    fn stop_game(&self, exe_name: &str) -> Result<(), ProcessError> {
        kill_all(named(&snapshot(), exe_name))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.game_running(exe_name) {
            if Instant::now() >= deadline {
                return Err(ProcessError::Other("the game did not stop in time".into()));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}
