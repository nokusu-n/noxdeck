//! The real `Processes`: launcher executable names in this install's root or
//! numeric version folder. A game inside the install is still only a game.

use super::{
    ProcessError, ProcessSnapshot, Processes, ExecutableName, CLIENT_EXE, GAME_LAUNCH_COMMAND, LAUNCHER_EXE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use crate::diagnostics::{Journal, Step, Window};
use std::sync::Arc;

#[derive(Default)]
pub struct RealProcesses {
    journal: Mutex<Option<Arc<Journal>>>,
    hidden: Mutex<Vec<(usize, u32)>>,
    launcher_ids: Mutex<(PathBuf, Vec<u32>)>,
    refreshing: Mutex<()>,
    #[cfg(test)]
    snapshot_count: std::sync::atomic::AtomicUsize,
}

impl RealProcesses {
    fn record(&self, step: Step) {
        if let Some(journal) = &*self.journal.lock().unwrap_or_else(|e| e.into_inner()) { journal.record(step); }
    }
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
                self.record(Step::Window(if command == 4 { Window::LauncherShownWithoutActivation } else { Window::LauncherShown }));
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
    exe(pid).is_some_and(|path| launcher_executable(&path, install))
}

fn launcher_executable(path: &Path, install: &Path) -> bool {
    let dir = format!("{}\\", norm(install).trim_end_matches('\\'));
    let path = norm(path);
    let Some(relative) = path.strip_prefix(&dir) else { return false; };
    let parts: Vec<_> = relative.split('\\').collect();
    let name = match parts.as_slice() {
        [name] => *name,
        [version, name] if version.strip_prefix("battle.net.").is_some_and(|build|
            !build.is_empty() && build.bytes().all(|b| b.is_ascii_digit())) => *name,
        _ => return false,
    };
    name.starts_with("battle.net") && name.ends_with(".exe")
}

fn stop_inventory<'a>(paths: impl IntoIterator<Item = &'a Path>, install: &Path)
    -> (BTreeMap<ExecutableName, usize>, BTreeMap<ExecutableName, usize>)
{
    let dir = format!("{}\\", norm(install).trim_end_matches('\\'));
    let (mut selected, mut excluded) = (BTreeMap::new(), BTreeMap::new());
    for path in paths {
        if !norm(path).starts_with(&dir) { continue; }
        if let Some(name) = ExecutableName::from_path(path) {
            let counts = if launcher_executable(path, install) { &mut selected } else { &mut excluded };
            *counts.entry(name).or_insert(0) += 1;
        }
    }
    (selected, excluded)
}

type Hwnd = *mut std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Hwnd;
    fn QueryFullProcessImageNameW(process: Hwnd, flags: u32, name: *mut u16, size: *mut u32)
        -> i32;
    fn CloseHandle(handle: Hwnd) -> i32;
    fn TerminateProcess(process: Hwnd, exit_code: u32) -> i32;
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

    #[derive(serde::Deserialize)]
    struct Layout { path: PathBuf, launcher: bool }
    fn layout() -> Vec<Layout> {
        serde_json::from_str(include_str!("../../tests/fixtures/launcher-process-layout.json")).unwrap()
    }

    #[test]
    fn launcher_names_in_root_and_numeric_version_folders_are_all_found() {
        let install = Path::new("C:/StandIn/Install");
        for entry in layout() {
            assert_eq!(launcher_executable(&entry.path, install), entry.launcher, "{}", entry.path.display());
            assert_eq!(scoped_executable(101, install, |_| Some(entry.path)), entry.launcher);
        }
        for name in ["BATTLE.NET NEW HELPER.EXE", "battle.net-x.exe", "Battle.net.exe"] {
            assert!(launcher_executable(&install.join(name), install));
            assert!(launcher_executable(&install.join("Battle.net.12345").join(name), install));
            assert!(!launcher_executable(&install.join("Games").join(name), install));
            assert!(!launcher_executable(&Path::new("C:/Elsewhere").join(name), install));
        }
        for name in ["Battle.net Helper.dll", "Battle.net.exe.old", "NotBattle.net.exe", "Overwatch.exe"] {
            assert!(!launcher_executable(&install.join(name), install));
        }
    }

    #[test]
    fn nested_games_are_never_selected_stopped_or_hidden_as_the_launcher() {
        let install = Path::new("C:/StandIn/Install");
        let layout = layout();
        let selected: Vec<_> = layout.iter().enumerate().filter_map(|(i, entry)|
            scoped_executable(i as u32 + 100, install, |_| Some(entry.path.clone())).then_some(i as u32 + 100)
        ).collect();
        for (i, entry) in layout.iter().enumerate().filter(|(_, entry)| !entry.launcher) {
            assert!(!selected.contains(&(i as u32 + 100)));
            assert!(!scoped_owner(i as u32 + 100, i as u32 + 100, &selected), "game window was selected for hiding");
            assert!(!scoped_executable(i as u32 + 100, install, |_| Some(entry.path.clone())), "last window ownership check must reject the game too");
        }
        let mut snapshots = std::collections::VecDeque::from([selected.clone(), vec![]]);
        let mut stopped = Vec::new();
        assert_eq!(stop_until_gone(false, Duration::from_millis(250), || snapshots.pop_front().unwrap_or_default(),
            |pid| { stopped.push(pid); Ok(()) }, |_| {}), Ok(()));
        assert_eq!(stopped, selected);
        assert_eq!(stopped.len(), layout.iter().filter(|entry| entry.launcher).count());
    }

    #[test]
    fn diagnostics_identify_selected_and_excluded_executable_names_and_counts_without_paths() {
        let layout = layout();
        let (selected, excluded) = stop_inventory(layout.iter().map(|entry| entry.path.as_path()), Path::new("C:/StandIn/Install"));
        assert_eq!(selected.values().sum::<usize>(), layout.iter().filter(|entry| entry.launcher).count());
        assert_eq!(selected[&ExecutableName::from_path(Path::new("Battle.net Extra Helper.exe")).unwrap()], 2);
        assert_eq!(excluded[&ExecutableName::from_path(Path::new("Overwatch.exe")).unwrap()], 3);
        assert_eq!(excluded[&ExecutableName::from_path(Path::new("Battle.net Extra Helper.exe")).unwrap()], 1);
        let journal = Journal::default();
        journal.record(Step::LauncherStopProcesses { selected: true, executables: selected });
        journal.record(Step::LauncherStopProcesses { selected: false, executables: excluded });
        let text = journal.text();
        assert!(text.contains("Launcher directory processes selected: count=8"));
        assert!(text.contains("Launcher directory processes excluded: count=9"));
        assert!(text.contains("Battle.net Extra Helper.exe=2"));
        assert!(text.contains("Overwatch.exe=3"));
        assert!(text.contains("Battle.net Extra Helper.exe=1"));
        assert!(!text.contains("StandIn"));
        assert!(!text.contains("C:"));
    }

    #[test]
    fn diagnostic_executable_names_cannot_include_paths_or_account_text() {
        for path in ["C:/StandIn/private/Battle.net placeholder@example.com.exe", "C:/StandIn/private/ExamplePlayer#12345.exe", "C:/StandIn/private/multi\nline.exe"] {
            let name = ExecutableName::from_path(Path::new(path)).unwrap();
            assert_eq!(name.file_name(), "<masked executable>");
        }
    }

    #[test]
    fn failed_kills_of_lingering_helpers_succeed_when_the_list_becomes_empty() {
        let mut snapshots = std::collections::VecDeque::from([vec![101, 102], vec![102], vec![]]);
        let mut attempts = 0;
        let result = stop_until_gone(false, Duration::from_millis(350), || snapshots.pop_front().unwrap_or_default(), |_| { attempts += 1; Err(87) }, |_| {});
        assert_eq!(result, Ok(()));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn a_failed_game_kill_succeeds_when_the_game_leaves_and_retries_while_alive() {
        let mut snapshots = std::collections::VecDeque::from([vec![101], vec![101], vec![]]);
        let mut attempts = 0;
        let mut steps = Vec::new();
        let result = stop_until_gone(true, Duration::from_millis(350),
            || snapshots.pop_front().unwrap_or_default(),
            |_| { attempts += 1; Err(5) }, |step| steps.push(step));
        assert_eq!(result, Ok(()));
        assert_eq!(attempts, 2);
        assert!(steps.iter().all(|step| matches!(step, Step::StopAttempt { game: true, .. } | Step::StopSnapshot { game: true, .. })));
        assert!(matches!(steps.last(), Some(Step::StopSnapshot { game: true, remaining: 0 })));
    }

    #[test]
    fn only_access_refused_to_a_process_still_alive_at_the_bound_means_administrator() {
        for (code, expected) in [(5, ProcessError::AccessDenied), (87, ProcessError::TimedOut)] {
            let started = Instant::now();
            let result = stop_until_gone(false, Duration::from_millis(30), || vec![101], |_| Err(code), |_| {});
            assert_eq!(result, Err(expected));
            assert!(started.elapsed() >= Duration::from_millis(30));
        }
        let mut snapshots = std::collections::VecDeque::from([vec![101, 102], vec![102], vec![102]]);
        let result = stop_until_gone(false, Duration::from_millis(150), || snapshots.pop_front().unwrap_or_else(|| vec![102]), |pid| Err(if pid == 101 { 5 } else { 87 }), |_| {});
        assert_eq!(result, Err(ProcessError::TimedOut), "a departed denied process is no evidence about a survivor");
    }

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
            dir.join(LAUNCHER_EXE)
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
    sys.processes()
        .values()
        .filter(|p| p.exe().is_some_and(|e| launcher_executable(e, install)))
        .collect()
}

fn named<'a>(sys: &'a System, exe_name: &str) -> Vec<&'a sysinfo::Process> {
    sys.processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe_name))
        .collect()
}

fn kill(pid: u32) -> Result<(), i32> {
    unsafe {
        let process = OpenProcess(1, 0, pid);
        if process.is_null() { return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0)); }
        let ok = TerminateProcess(process, 1) != 0;
        let error = (!ok).then(|| std::io::Error::last_os_error().raw_os_error().unwrap_or(0));
        CloseHandle(process);
        error.map_or(Ok(()), Err)
    }
}
fn game_windows(pids: &[u32]) -> Vec<u32> {
    let mut windows = Vec::<usize>::new();
    unsafe { EnumWindows(collect_window, &mut windows as *mut _ as isize); }
    let mut visible: Vec<_> = windows.into_iter().filter_map(|w| {
        let pid = owner(w as Hwnd);
        (pids.contains(&pid) && unsafe { IsWindowVisible(w as Hwnd) != 0 }).then_some(pid)
    }).collect();
    visible.sort_unstable();
    visible.dedup();
    visible
}

impl Processes for RealProcesses {
    fn set_journal(&self, journal: Arc<Journal>) {
        *self.journal.lock().unwrap_or_else(|e| e.into_inner()) = Some(journal);
    }
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
        let before = hidden.len();
        hidden.retain(|(handle, remembered)| {
            scoped_owner(owner(*handle as Hwnd), *remembered, &allowed)
        });
        if hidden.len() < before { self.record(Step::Window(Window::LauncherDestroyed)); }
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
                if let Some(journal) = &*self.journal.lock().unwrap_or_else(|e| e.into_inner()) { journal.hide_window(handle, pid); }
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
            launcher_count: install.map_or(0, |dir| inside(&sys, dir).len()),
            game_running: !game_pids.is_empty(),
            game_window_pids: game_windows(&game_pids),
            game_pids,
        }
    }

    fn launcher_running(&self, install: &Path) -> bool {
        !inside(&snapshot(), install).is_empty()
    }

    fn stop_launcher(&self, install: &Path, timeout: Duration) -> Result<(), ProcessError> {
        let mut previous = None;
        stop_until_gone(false, timeout, || {
            let sys = snapshot();
            let matched = inside(&sys, install);
            let inventory = stop_inventory(sys.processes().values().filter_map(|process| process.exe()), install);
            if previous.as_ref() != Some(&inventory) {
                self.record(Step::LauncherStopProcesses { selected: true, executables: inventory.0.clone() });
                self.record(Step::LauncherStopProcesses { selected: false, executables: inventory.1.clone() });
                previous = Some(inventory);
            }
            matched.iter().map(|p| p.pid().as_u32()).collect()
        }, kill, |step| self.record(step))
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
        stop_until_gone(true, Duration::from_secs(10), || {
            named(&snapshot(), exe_name).iter().map(|process| process.pid().as_u32()).collect()
        }, kill, |step| self.record(step))
    }
}

fn stop_until_gone(
    game: bool, timeout: Duration, mut pids: impl FnMut() -> Vec<u32>,
    mut kill: impl FnMut(u32) -> Result<(), i32>, mut record: impl FnMut(Step),
) -> Result<(), ProcessError> {
    let deadline = Instant::now() + timeout;
    let mut denied = std::collections::HashSet::new();
    let mut count = None;
    loop {
        let alive = pids();
        if count != Some(alive.len()) { record(Step::StopSnapshot { game, remaining: alive.len() }); count = Some(alive.len()); }
        denied.retain(|pid| alive.contains(pid));
        if alive.is_empty() { return Ok(()); }
        if Instant::now() >= deadline {
            return Err(if denied.is_empty() { ProcessError::TimedOut } else { ProcessError::AccessDenied });
        }
        for pid in alive {
            let result = kill(pid);
            record(Step::StopAttempt { game, success: result.is_ok(), error_code: result.as_ref().err().copied() });
            if result == Err(5) { denied.insert(pid); }
        }
        std::thread::sleep(Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())));
    }
}
