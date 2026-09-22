use std::ffi::c_void;
use windows_sys::core::BOOL;
use windows_sys::Win32::{Foundation::*, Security::SECURITY_ATTRIBUTES, System::Threading::*};

pub type CreateProcess = unsafe extern "system" fn(
    *const u16,
    *mut u16,
    *const SECURITY_ATTRIBUTES,
    *const SECURITY_ATTRIBUTES,
    BOOL,
    u32,
    *const c_void,
    *const u16,
    *const STARTUPINFOW,
    *mut PROCESS_INFORMATION,
) -> BOOL;

unsafe extern "system" {
    pub fn DetourTransactionBegin() -> i32;
    pub fn DetourTransactionAbort() -> i32;
    pub fn DetourTransactionCommit() -> i32;
    pub fn DetourUpdateThread(thread: HANDLE) -> i32;
    pub fn DetourAttach(target: *mut *mut c_void, replacement: *mut c_void) -> i32;
    pub fn DetourRestoreAfterWith() -> BOOL;
    pub fn DetourCreateProcessWithDllExW(
        app: *const u16,
        cmd: *mut u16,
        pa: *const SECURITY_ATTRIBUTES,
        ta: *const SECURITY_ATTRIBUTES,
        inherit: BOOL,
        flags: u32,
        env: *const c_void,
        cwd: *const u16,
        startup: *const STARTUPINFOW,
        info: *mut PROCESS_INFORMATION,
        dll: *const u8,
        original: Option<CreateProcess>,
    ) -> BOOL;
}
