use launcher_core::*;
use launcher_detours::*;
use std::{
    ffi::{c_void, CString},
    ptr,
    sync::OnceLock,
};
use windows_sys::core::BOOL;
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    System::{LibraryLoader::DisableThreadLibraryCalls, Threading::*},
};

static mut ORIGINAL: *mut c_void = ptr::null_mut();
static mut ORIGINAL_A: *mut c_void = ptr::null_mut();
static SESSION: OnceLock<Option<Session>> = OnceLock::new();

unsafe fn string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

#[no_mangle]
pub extern "system" fn DetourOrdinalOne() {}

#[no_mangle]
/// Windows loader entry point; only installs native hooks during process attach.
///
/// # Safety
/// Called by the Windows loader with a valid module handle and loader-owned arguments.
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason != 1 {
        return 1;
    }
    DisableThreadLibraryCalls(module);
    DetourRestoreAfterWith();
    ORIGINAL = CreateProcessW as *mut c_void;
    ORIGINAL_A = CreateProcessA as *mut c_void;
    if DetourTransactionBegin() != 0 {
        return 0;
    }
    if DetourUpdateThread(GetCurrentThread()) != 0
        || DetourAttach(
            ptr::addr_of_mut!(ORIGINAL),
            hooked_create_process as *mut c_void,
        ) != 0
        || DetourAttach(
            ptr::addr_of_mut!(ORIGINAL_A),
            hooked_create_process_a as *mut c_void,
        ) != 0
    {
        DetourTransactionAbort();
        return 0;
    }
    (DetourTransactionCommit() == 0) as BOOL
}

unsafe extern "system" fn hooked_create_process(
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
) -> BOOL {
    let original: CreateProcess = std::mem::transmute(ORIGINAL);
    let Some(session) = SESSION.get_or_init(|| Session::from_env().ok()).as_ref() else {
        // A missing/corrupt session must never silently run the old compiler.
        SetLastError(ERROR_INVALID_DATA);
        return 0;
    };
    let raw = string(cmd);
    let args = match split_args(&raw, false) {
        Ok(args) => args,
        Err(_) => return original(app, cmd, pa, ta, inherit, flags, env, cwd, startup, info),
    };
    let application = if app.is_null() {
        args.first().cloned().unwrap_or_default()
    } else {
        string(app)
    };
    if let Some(compiler) =
        compiler_args(&args, session).filter(|_| same_path(&application, &session.original_dotnet))
    {
        let mut replacement = vec![
            session.proxy.to_string_lossy().into_owned(),
            "--session".into(),
            session
                .directory
                .join("session.json")
                .to_string_lossy()
                .into_owned(),
            "--".into(),
        ];
        replacement.extend(compiler);
        let replacement = command(&replacement);
        session.log("redirect", &replacement);
        return original(
            wide(&session.proxy).as_ptr(),
            wide(&replacement).as_mut_ptr(),
            pa,
            ta,
            inherit,
            flags,
            env,
            cwd,
            startup,
            info,
        );
    }
    let is_compiler_shell = std::path::Path::new(&application)
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("cmd.exe"))
        && raw.replace('/', "\\").to_lowercase().contains(
            &session
                .original_csc
                .to_string_lossy()
                .replace('/', "\\")
                .to_lowercase(),
        );
    if is_compiler_shell
        || same_path(&application, &session.bee)
        || session
            .extra_propagate
            .iter()
            .any(|p| same_path(&application, p))
    {
        session.log("inject-child", &application);
        let Ok(dll) = CString::new(session.hook.to_string_lossy().as_bytes()) else {
            SetLastError(ERROR_INVALID_NAME);
            return 0;
        };
        // Unity/Bee may supply their own environment block. Always propagate this
        // session explicitly, including when the caller stripped inherited variables.
        let child_env = merge_environment(env, flags, session);
        let (env, flags) = if let Some(ref block) = child_env {
            (block.as_ptr().cast(), flags | CREATE_UNICODE_ENVIRONMENT)
        } else {
            (env, flags)
        };
        let result = DetourCreateProcessWithDllExW(
            app,
            cmd,
            pa,
            ta,
            inherit,
            flags,
            env,
            cwd,
            startup,
            info,
            dll.as_ptr().cast(),
            Some(original),
        );
        let error = GetLastError();
        if result == 0 {
            session.log("inject-error", error.to_string());
        }
        SetLastError(error);
        return result;
    }
    original(app, cmd, pa, ta, inherit, flags, env, cwd, startup, info)
}

unsafe fn ansi_wide(p: *const u8) -> Vec<u16> {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_ACP};
    if p.is_null() {
        return Vec::new();
    }
    let n = MultiByteToWideChar(CP_ACP, 0, p, -1, ptr::null_mut(), 0);
    let mut result = vec![0; n as usize];
    if n > 0 {
        MultiByteToWideChar(CP_ACP, 0, p, -1, result.as_mut_ptr(), n);
    }
    result
}
fn nullable(v: &[u16]) -> *const u16 {
    if v.is_empty() {
        ptr::null()
    } else {
        v.as_ptr()
    }
}

unsafe fn merge_environment(env: *const c_void, flags: u32, session: &Session) -> Option<Vec<u16>> {
    if env.is_null() {
        return None;
    }
    let mut entries: Vec<Vec<u16>> = Vec::new();
    if flags & CREATE_UNICODE_ENVIRONMENT != 0 {
        let mut p = env.cast::<u16>();
        while *p != 0 {
            let mut n = 0;
            while *p.add(n) != 0 {
                n += 1;
            }
            entries.push(std::slice::from_raw_parts(p, n).to_vec());
            p = p.add(n + 1);
        }
    } else {
        let mut p = env.cast::<u8>();
        while *p != 0 {
            let mut entry = ansi_wide(p);
            entry.pop();
            entries.push(entry);
            while *p != 0 {
                p = p.add(1);
            }
            p = p.add(1);
        }
    }
    let prefix = format!("{SESSION_ENV}=");
    entries.retain(|e| {
        !String::from_utf16_lossy(e)
            .to_uppercase()
            .starts_with(&prefix)
    });
    let mut entry = wide(format!(
        "{prefix}{}",
        session.directory.join("session.json").display()
    ));
    entry.pop();
    entries.push(entry);
    entries.sort_by_key(|e| String::from_utf16_lossy(e).to_uppercase());
    let mut block = Vec::new();
    for entry in entries {
        block.extend(entry);
        block.push(0);
    }
    block.push(0);
    Some(block)
}

unsafe extern "system" fn hooked_create_process_a(
    app: *const u8,
    cmd: *mut u8,
    pa: *const SECURITY_ATTRIBUTES,
    ta: *const SECURITY_ATTRIBUTES,
    inherit: BOOL,
    flags: u32,
    env: *const c_void,
    cwd: *const u8,
    startup: *const STARTUPINFOA,
    info: *mut PROCESS_INFORMATION,
) -> BOOL {
    // Leave unrelated ANSI calls byte-for-byte unchanged, including STARTUPINFOEX.
    type CreateProcessAType = unsafe extern "system" fn(
        *const u8,
        *mut u8,
        *const SECURITY_ATTRIBUTES,
        *const SECURITY_ATTRIBUTES,
        BOOL,
        u32,
        *const c_void,
        *const u8,
        *const STARTUPINFOA,
        *mut PROCESS_INFORMATION,
    ) -> BOOL;
    let original_a: CreateProcessAType = std::mem::transmute(ORIGINAL_A);
    let app_w = ansi_wide(app);
    let cmd_w = ansi_wide(cmd);
    let app_text = string(nullable(&app_w));
    let cmd_text = string(nullable(&cmd_w));
    if let Some(session) = SESSION.get_or_init(|| Session::from_env().ok()).as_ref() {
        let candidate = format!("{app_text} {cmd_text}")
            .replace('/', "\\")
            .to_lowercase();
        let matches = [&session.original_csc, &session.bee]
            .into_iter()
            .chain(session.extra_propagate.iter())
            .any(|p| candidate.contains(&p.to_string_lossy().replace('/', "\\").to_lowercase()));
        if !matches {
            return original_a(app, cmd, pa, ta, inherit, flags, env, cwd, startup, info);
        }
    }
    if startup.is_null() {
        SetLastError(ERROR_INVALID_PARAMETER);
        return 0;
    }
    // STARTUPINFOEX has trailing attribute-list data. Do not truncate it.
    if flags & EXTENDED_STARTUPINFO_PRESENT != 0 {
        SetLastError(ERROR_NOT_SUPPORTED);
        return 0;
    }
    let app = ansi_wide(app);
    let mut cmd = ansi_wide(cmd);
    let cwd = ansi_wide(cwd);
    let reserved = ansi_wide((*startup).lpReserved);
    let desktop = ansi_wide((*startup).lpDesktop);
    let title = ansi_wide((*startup).lpTitle);
    let mut startup_w: STARTUPINFOW = std::mem::transmute_copy(&*startup);
    startup_w.lpReserved = nullable(&reserved) as *mut _;
    startup_w.lpDesktop = nullable(&desktop) as *mut _;
    startup_w.lpTitle = nullable(&title) as *mut _;
    let mut environment = Vec::<u16>::new();
    let mut flags = flags;
    let mut env = env;
    if !env.is_null() && flags & CREATE_UNICODE_ENVIRONMENT == 0 {
        let mut entry = env.cast::<u8>();
        while *entry != 0 {
            environment.extend(ansi_wide(entry));
            while *entry != 0 {
                entry = entry.add(1);
            }
            entry = entry.add(1);
        }
        environment.push(0);
        if environment.len() == 1 {
            environment.push(0);
        }
        env = environment.as_ptr().cast();
        flags |= CREATE_UNICODE_ENVIRONMENT;
    }
    hooked_create_process(
        nullable(&app),
        if cmd.is_empty() {
            ptr::null_mut()
        } else {
            cmd.as_mut_ptr()
        },
        pa,
        ta,
        inherit,
        flags,
        env,
        nullable(&cwd),
        &startup_w,
        info,
    )
}
