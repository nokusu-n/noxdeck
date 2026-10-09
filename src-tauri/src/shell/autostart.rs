//! One current-user Run value. Tests supply another name and a nonexistent exe.
use std::{ffi::c_void, io, path::Path};
type Hkey = *mut c_void;
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const HKCU: Hkey = (0x80000001u32 as i32 as isize) as Hkey;
#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        key: Hkey,
        subkey: *const u16,
        options: u32,
        access: u32,
        result: *mut Hkey,
    ) -> i32;
    fn RegCreateKeyExW(
        key: Hkey,
        subkey: *const u16,
        reserved: u32,
        class: *mut u16,
        options: u32,
        access: u32,
        security: *mut c_void,
        result: *mut Hkey,
        disposition: *mut u32,
    ) -> i32;
    fn RegQueryValueExW(
        key: Hkey,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: Hkey,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}
struct Key(Hkey);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn checked(code: i32) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}
fn command(exe: &Path) -> String {
    format!("\"{}\" --tray", exe.display())
}

pub struct Entry {
    name: Vec<u16>,
}
impl Entry {
    pub fn new(name: &str) -> Self {
        assert!(!name.is_empty() && !name.contains('\0'));
        Self { name: wide(name) }
    }
    pub fn configured() -> Self {
        Self::new(&std::env::var("NOXDECK_AUTOSTART_NAME").unwrap_or_else(|_| "Noxdeck".into()))
    }
    pub fn read(&self) -> io::Result<Option<String>> {
        unsafe {
            let mut raw = std::ptr::null_mut();
            let status = RegOpenKeyExW(HKCU, wide(RUN).as_ptr(), 0, 1, &mut raw);
            if status == 2 {
                return Ok(None);
            }
            checked(status)?;
            let key = Key(raw);
            let (mut size, mut kind) = (0, 0);
            let status = RegQueryValueExW(
                key.0,
                self.name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            );
            if status == 2 {
                return Ok(None);
            }
            checked(status)?;
            // Presence is the switch's truth. A malformed value is repaired at release startup.
            if kind != 1 {
                return Ok(Some(String::new()));
            }
            let mut data = vec![0u16; (size as usize).div_ceil(2)];
            checked(RegQueryValueExW(
                key.0,
                self.name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                data.as_mut_ptr().cast(),
                &mut size,
            ))?;
            let data = &data[..size as usize / 2];
            let end = data.iter().position(|c| *c == 0).unwrap_or(data.len());
            Ok(Some(String::from_utf16_lossy(&data[..end])))
        }
    }
    pub fn set(&self, enabled: bool, exe: &Path) -> io::Result<()> {
        unsafe {
            let mut raw = std::ptr::null_mut();
            let status = if enabled {
                RegCreateKeyExW(
                    HKCU,
                    wide(RUN).as_ptr(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    2,
                    std::ptr::null_mut(),
                    &mut raw,
                    std::ptr::null_mut(),
                )
            } else {
                RegOpenKeyExW(HKCU, wide(RUN).as_ptr(), 0, 2, &mut raw)
            };
            if !enabled && status == 2 {
                return Ok(());
            }
            checked(status)?;
            let key = Key(raw);
            if enabled {
                let value = wide(&command(exe));
                checked(RegSetValueExW(
                    key.0,
                    self.name.as_ptr(),
                    0,
                    1,
                    value.as_ptr().cast(),
                    (value.len() * 2) as u32,
                ))
            } else {
                let status = RegDeleteValueW(key.0, self.name.as_ptr());
                if status == 2 {
                    Ok(())
                } else {
                    checked(status)
                }
            }
        }
    }
    pub fn repair(&self, exe: &Path) -> io::Result<()> {
        if self.read()?.is_some_and(|value| value != command(exe)) {
            self.set(true, exe)?;
        }
        Ok(())
    }
}
