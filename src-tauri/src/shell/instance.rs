//! One instance for the Windows user SID. A one-byte pipe asks it to show its window.
use super::ShellRequest;
use std::{ffi::c_void, io, sync::mpsc::Sender};
type RawHandle = *mut c_void;

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: RawHandle, owner: i32, name: *const u16) -> RawHandle;
    fn GetLastError() -> u32;
    fn GetCurrentProcess() -> RawHandle;
    fn CloseHandle(handle: RawHandle) -> i32;
    fn LocalFree(memory: RawHandle) -> RawHandle;
    fn CreateNamedPipeW(
        name: *const u16,
        access: u32,
        mode: u32,
        instances: u32,
        out: u32,
        input: u32,
        timeout: u32,
        attributes: RawHandle,
    ) -> RawHandle;
    fn ConnectNamedPipe(pipe: RawHandle, overlapped: RawHandle) -> i32;
    fn DisconnectNamedPipe(pipe: RawHandle) -> i32;
    fn ReadFile(
        handle: RawHandle,
        buffer: *mut u8,
        length: u32,
        read: *mut u32,
        overlapped: RawHandle,
    ) -> i32;
    fn WriteFile(
        handle: RawHandle,
        buffer: *const u8,
        length: u32,
        written: *mut u32,
        overlapped: RawHandle,
    ) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        attributes: RawHandle,
        creation: u32,
        flags: u32,
        template: RawHandle,
    ) -> RawHandle;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
    fn GetNamedPipeServerProcessId(pipe: RawHandle, pid: *mut u32) -> i32;
}
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(process: RawHandle, access: u32, token: *mut RawHandle) -> i32;
    fn GetTokenInformation(
        token: RawHandle,
        class: u32,
        buffer: RawHandle,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: RawHandle, string: *mut *mut u16) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn AllowSetForegroundWindow(pid: u32) -> i32;
}

struct Handle(RawHandle);
unsafe impl Send for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Handle {
    fn raw(&self) -> RawHandle {
        self.0
    }
    fn checked(raw: RawHandle) -> io::Result<Self> {
        if raw.is_null() || raw as isize == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn user_sid() -> io::Result<String> {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Handle::checked(token)?;
        let mut length = 0;
        GetTokenInformation(token.0, 1, std::ptr::null_mut(), 0, &mut length);
        let mut buffer = vec![0u64; (length as usize + 7) / 8];
        if GetTokenInformation(token.0, 1, buffer.as_mut_ptr().cast(), length, &mut length) == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = *(buffer.as_ptr() as *const RawHandle);
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(sid)
    }
}

pub struct InstanceGuard {
    _mutex: Handle,
}

impl InstanceGuard {
    /// `None` means the existing instance received the start and this process ends.
    pub fn acquire(requests: Sender<ShellRequest>, tray: bool) -> io::Result<Option<Self>> {
        let scope = std::env::var("NOXDECK_INSTANCE").unwrap_or_else(|_| "main".into());
        if scope.len() > 64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NOXDECK_INSTANCE is too long",
            ));
        }
        let suffix: String = scope.bytes().map(|b| format!("{b:02x}")).collect();
        let name = format!("NoxDeck-{}-{suffix}", user_sid()?);
        let pipe_name = wide(&format!(r"\\.\pipe\{name}"));
        unsafe {
            let mutex = CreateMutexW(
                std::ptr::null_mut(),
                0,
                wide(&format!("Global\\{name}")).as_ptr(),
            );
            let existed = GetLastError() == 183;
            let mutex = Handle::checked(mutex)?;
            if existed {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                let pipe = loop {
                    let raw = CreateFileW(
                        pipe_name.as_ptr(),
                        0x4000_0000,
                        0,
                        std::ptr::null_mut(),
                        3,
                        0,
                        std::ptr::null_mut(),
                    );
                    if let Ok(pipe) = Handle::checked(raw) {
                        break pipe;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(io::Error::last_os_error());
                    }
                    WaitNamedPipeW(pipe_name.as_ptr(), 100);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                };
                if !tray {
                    let mut pid = 0;
                    if GetNamedPipeServerProcessId(pipe.0, &mut pid) != 0 {
                        AllowSetForegroundWindow(pid);
                    }
                }
                let message = u8::from(!tray);
                let mut written = 0;
                if WriteFile(pipe.0, &message, 1, &mut written, std::ptr::null_mut()) == 0
                    || written != 1
                {
                    return Err(io::Error::last_os_error());
                }
                return Ok(None);
            }
            // Reject remote clients; only this user's local starts are relevant.
            let pipe = Handle::checked(CreateNamedPipeW(
                pipe_name.as_ptr(),
                1 | 0x0008_0000,
                8,
                1,
                1,
                1,
                0,
                std::ptr::null_mut(),
            ))?;
            std::thread::spawn(move || loop {
                if ConnectNamedPipe(pipe.raw(), std::ptr::null_mut()) == 0 && GetLastError() != 535
                {
                    break;
                }
                let mut message = 0;
                let mut read = 0;
                if ReadFile(pipe.raw(), &mut message, 1, &mut read, std::ptr::null_mut()) != 0
                    && read == 1
                    && message == 1
                {
                    if requests.send(ShellRequest::ShowWindow).is_err() {
                        break;
                    }
                }
                DisconnectNamedPipe(pipe.raw());
            });
            Ok(Some(Self { _mutex: mutex }))
        }
    }
}
