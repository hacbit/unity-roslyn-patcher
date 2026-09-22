use launcher_core::*;
mod ide;
use launcher_detours::*;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    ptr,
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*, System::Threading::*};

const HELP: &str = "unity-launcher [--project DIR] [--config FILE] [--wait] [--dry-run] [--rebuild] [-- Unity args...]\n\
  Default project: search current directory and parents.\n\
  Config: --config, project/unity-launcher.json, or beside this EXE.\n\
  --self-test: test injected parent -> child -> compiler using a C# 12 fixture.\n\
  --wait: return Unity's exit code (use with -batchmode -quit for CI).\n\
  Compiler/config changes archive Library/Bee and Library/ScriptAssemblies.\n\
  Windows x64 / Unity 2022.3 prototype; Unity must be closed before launch.";

fn absolute(path: &Path) -> Result<PathBuf> {
    let p = fs::canonicalize(path)?;
    Ok(PathBuf::from(
        p.to_string_lossy().trim_start_matches("\\\\?\\"),
    ))
}
fn read_config(path: &Path, project: &Path) -> Result<Config> {
    let mut c: Config = serde_json::from_slice(&fs::read(path)?)?;
    let base = path.parent().unwrap_or(Path::new("."));
    c.unity = if c.unity.as_os_str().is_empty() {
        find_unity(project)?
    } else {
        absolute(&base.join(&c.unity))?
    };
    c.dotnet = absolute(&base.join(&c.dotnet))?;
    c.csc = absolute(&base.join(&c.csc))?;
    if c.lang_version.is_empty()
        || !c
            .lang_version
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
    {
        return Err("lang_version must be fixed, e.g. 12 or 14.0 (not latest/preview)".into());
    }
    Ok(c)
}
fn find_unity(project: &Path) -> Result<PathBuf> {
    let text = fs::read_to_string(project.join("ProjectSettings/ProjectVersion.txt"))?;
    let version = text
        .lines()
        .find_map(|l| l.strip_prefix("m_EditorVersion:"))
        .ok_or("ProjectVersion.txt has no m_EditorVersion")?
        .trim();
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.')
    {
        return Err("Invalid Unity version".into());
    }
    let mut roots = vec![PathBuf::from("D:/UnityEditors")];
    if let Some(root) = std::env::var_os("UNITY_EDITOR_ROOT") {
        roots.insert(0, root.into());
    }
    if let Some(root) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(root).join("Unity/Hub/Editor"));
    }
    for root in roots {
        let exe = root.join(version).join("Editor/Unity.exe");
        if exe.is_file() {
            return absolute(&exe);
        }
    }
    Err(format!("Cannot find Unity {version}; set unity in config or UNITY_EDITOR_ROOT").into())
}
fn project_from(start: &Path) -> Result<PathBuf> {
    for p in start.ancestors() {
        if p.join("ProjectSettings/ProjectVersion.txt").is_file() && p.join("Assets").is_dir() {
            return absolute(p);
        }
    }
    Err("No Unity project found. Use --project <directory>.".into())
}
fn config_from(project: &Path, bin: &Path) -> Result<PathBuf> {
    [
        project.join("unity-launcher.json"),
        bin.join("unity-launcher.json"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or_else(|| "No unity-launcher.json found. Use --config <file>.".into())
}

fn runtime_directory(bin: &Path) -> Result<PathBuf> {
    let manifest = bin.join("runtime.json");
    if !manifest.exists() {
        return Ok(bin.to_path_buf());
    }
    let data: serde_json::Value = serde_json::from_slice(&fs::read(manifest)?)?;
    let directory = Path::new(
        data.get("directory")
            .and_then(|v| v.as_str())
            .ok_or("Invalid runtime.json")?,
    );
    if directory.as_os_str().is_empty()
        || directory
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Runtime directory must be a relative path without traversal".into());
    }
    absolute(&bin.join(directory))
}
fn verify(c: &Config) -> Result<()> {
    let parent = c.csc.parent().ok_or("csc directory missing")?;
    for path in [
        &c.unity,
        &c.dotnet,
        &c.csc,
        &parent.join("csc.runtimeconfig.json"),
        &parent.join("Microsoft.CodeAnalysis.dll"),
        &parent.join("Microsoft.CodeAnalysis.CSharp.dll"),
    ] {
        if !path.is_file() {
            return Err(format!("Missing file: {}", path.display()).into());
        }
    }
    let output = Command::new(&c.dotnet)
        .arg("exec")
        .arg(&c.csc)
        .arg("-version")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Compiler preflight failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    println!("Roslyn: {}", String::from_utf8_lossy(&output.stdout).trim());
    let output = Command::new(&c.dotnet)
        .arg("exec")
        .arg(&c.csc)
        .arg("-langversion:?")
        .output()?;
    let versions = String::from_utf8_lossy(&output.stdout);
    fn normalized(v: &str) -> &str {
        v.strip_suffix(".0").unwrap_or(v)
    }
    if !versions.lines().any(|line| {
        line.split_whitespace().next().map(normalized) == Some(normalized(&c.lang_version))
    }) {
        return Err(format!("Compiler does not list language version {}", c.lang_version).into());
    }
    Ok(())
}
fn fingerprint(session: &Session) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update(serde_json::to_vec(&session.config)?);
    let mut files: Vec<PathBuf> = fs::read_dir(session.config.csc.parent().unwrap())?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    files.retain(|p| p.is_file());
    files.extend([
        session.hook.clone(),
        session.proxy.clone(),
        session.config.dotnet.clone(),
        std::env::current_exe()?,
    ]);
    files.sort();
    for path in files {
        hash.update(path.to_string_lossy().as_bytes());
        let mut f = fs::File::open(path)?;
        let mut buffer = [0; 65536];
        loop {
            let n = f.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn project_lock(project: &Path) -> Result<Handle> {
    // Unity keeps this file open while the project is running. Hold it until launch.
    fs::create_dir_all(project.join("Temp"))?;
    let file = wide(project.join("Temp/UnityLockfile"));
    let h = unsafe {
        CreateFileW(
            file.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(format!(
            "Project is open or its lock is inaccessible: {} ({})",
            project.display(),
            std::io::Error::last_os_error()
        )
        .into());
    }
    Ok(Handle(h))
}

fn cache(session: &Session, force: bool) -> Result<()> {
    let root = session.project.join("Library/RoslynLauncher");
    let stamp = root.join("compiler.sha256");
    let new = fingerprint(session)?;
    if force || fs::read_to_string(&stamp).ok().as_deref() != Some(&new) {
        let archive = root
            .join("cache-backups")
            .join(session.directory.file_name().unwrap());
        fs::create_dir_all(&archive)?;
        for name in ["Bee", "ScriptAssemblies"] {
            let from = session.project.join("Library").join(name);
            if from.exists() {
                // Only fixed children of this project's Library, never recursive deletion.
                use std::os::windows::fs::MetadataExt;
                if fs::symlink_metadata(&from)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT
                    != 0
                {
                    return Err("Refusing to move a linked cache directory".into());
                }
                println!(
                    "Archive cache: {} -> {}",
                    from.display(),
                    archive.join(name).display()
                );
                fs::rename(&from, archive.join(name))?;
            }
        }
        fs::write(stamp, new)?;
    }
    Ok(())
}

fn launch(session: &Session, executable: &Path, args: Vec<String>, wait: bool) -> Result<i32> {
    let session_path = session.directory.join("session.json");
    fs::write(&session_path, serde_json::to_vec_pretty(session)?)?;
    std::env::set_var(SESSION_ENV, &session_path);
    let dll = session.hook.to_string_lossy();
    if !dll.is_ascii() {
        return Err("Detours prototype requires an ASCII tool installation path".into());
    }
    let dll = CString::new(dll.as_bytes())?;
    let mut argv = vec![executable.to_string_lossy().into_owned()];
    argv.extend(args);
    let mut cmd = wide(command(&argv));
    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        DetourCreateProcessWithDllExW(
            wide(executable).as_ptr(),
            cmd.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            0,
            ptr::null(),
            wide(&session.project).as_ptr(),
            &startup,
            &mut info,
            dll.as_ptr().cast(),
            None,
        )
    };
    if ok == 0 {
        return Err(format!(
            "Injected launch failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    let process = Handle(info.hProcess);
    let _thread = Handle(info.hThread);
    println!(
        "Started PID {}. Logs: {}",
        info.dwProcessId,
        session.directory.display()
    );
    if !wait {
        return Ok(0);
    }
    unsafe {
        if WaitForSingleObject(process.0, INFINITE) != WAIT_OBJECT_0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut code = 1;
        if GetExitCodeProcess(process.0, &mut code) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(code as i32)
    }
}

fn run() -> Result<i32> {
    let bin = std::env::current_exe()?.parent().unwrap().to_path_buf();
    let (mut project, mut config, mut wait, mut dry, mut test, mut rebuild) =
        (None, None, false, false, false, false);
    let mut pass = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(0);
            }
            "--project" => {
                project = Some(PathBuf::from(args.next().ok_or("Missing project path")?))
            }
            "--config" => config = Some(PathBuf::from(args.next().ok_or("Missing config path")?)),
            "--wait" => wait = true,
            "--dry-run" => dry = true,
            "--self-test" => test = true,
            "--rebuild" => rebuild = true,
            "--" => {
                pass.extend(args);
                break;
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    let cwd = std::env::current_dir()?;
    let project = if test {
        absolute(&cwd)?
    } else {
        project_from(&project.unwrap_or(cwd))?
    };
    let config_path = absolute(&match config {
        Some(p) => p,
        None => config_from(&project, &bin)?,
    })?;
    let config = read_config(&config_path, &project)?;
    verify(&config)?;
    let runtime = runtime_directory(&bin)?;
    let data = config.unity.parent().unwrap().join("Data");
    let id = format!(
        "{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        std::process::id()
    );
    let directory = if test {
        project.join("work/probe").join(id)
    } else {
        project.join("Library/RoslynLauncher/sessions").join(id)
    };
    let mut session = Session {
        config,
        project,
        directory,
        hook: absolute(&runtime.join("unity_roslyn_hook.dll"))?,
        proxy: absolute(&runtime.join("compiler-proxy.exe"))?,
        original_dotnet: data.join("NetCoreRuntime/dotnet.exe"),
        original_csc: data.join("DotNetSdkRoslyn/csc.dll"),
        bee: data.join("bee_backend.exe"),
        extra_propagate: vec![],
    };
    println!(
        "Unity: {}\nProject: {}\nC#: {}",
        session.config.unity.display(),
        session.project.display(),
        session.config.lang_version
    );
    for p in [
        &session.original_dotnet,
        &session.original_csc,
        &session.bee,
    ] {
        if !p.is_file() {
            return Err(format!("Unsupported Unity layout: {}", p.display()).into());
        }
    }
    if dry {
        println!("Preflight OK; no project changes or launch.");
        return Ok(0);
    }
    if !session.hook.to_string_lossy().is_ascii() {
        return Err("Detours prototype requires an ASCII tool installation path".into());
    }
    fs::create_dir_all(&session.directory)?;
    if test {
        let probe = absolute(&bin.join("chain-probe.exe"))?;
        session.extra_propagate.push(probe.clone());
        let source = session.directory.join("Syntax.cs");
        fs::write(&source, "namespace Probe; public class Modern(int value) { public int Value => value; public int[] Items => [1, 2, 3]; }\n")?;
        let output = session.directory.join("Syntax.dll");
        let refs = data.join("NetStandard/ref/2.1.0/netstandard.dll");
        if !refs.is_file() {
            return Err(format!("Missing probe reference: {}", refs.display()).into());
        }
        let rsp = [
            "/target:library".into(),
            format!("/r:{}", refs.display()),
            format!("/out:{}", output.display()),
            source.to_string_lossy().into_owned(),
        ];
        fs::write(
            session.directory.join("probe.rsp"),
            rsp.iter().map(|s| quote(s)).collect::<Vec<_>>().join("\n"),
        )?;
        let old = Command::new(&session.original_dotnet)
            .arg("exec")
            .arg(&session.original_csc)
            .args(["/noconfig", "/nostdlib"])
            .arg(format!(
                "@{}",
                session.directory.join("probe.rsp").display()
            ))
            .output()?;
        fs::write(
            session.directory.join("baseline.txt"),
            [old.stdout, old.stderr].concat(),
        )?;
        if old.status.success() {
            return Err("Negative control unexpectedly compiled the C# 12 fixture".into());
        }
        let code = launch(&session, &probe, vec![], true)?;
        if code != 0 || !output.is_file() {
            return Err(format!("Injection chain probe failed (exit {code})").into());
        }
        println!("PASS: old compiler rejected fixture; injected parent -> child -> replacement compiler produced {}", output.display());
        let bad_source = session.directory.join("Broken.cs");
        fs::write(&bad_source, "class Broken { ??? }\n")?;
        let bad_output = session.directory.join("Broken.dll");
        let bad_rsp = [
            "/target:library".into(),
            format!("/r:{}", refs.display()),
            format!("/out:{}", bad_output.display()),
            bad_source.to_string_lossy().into_owned(),
        ];
        fs::write(
            session.directory.join("bad.rsp"),
            bad_rsp
                .iter()
                .map(|s| quote(s))
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
        let code = launch(&session, &probe, vec!["--bad".into()], true)?;
        if code == 0 || bad_output.exists() {
            return Err("Compiler error was not propagated correctly".into());
        }
        println!("PASS: explicit child environment repaired; compiler errors returned a nonzero exit code.");
        return Ok(0);
    }
    let lock = project_lock(&session.project)?;
    ide::install(&session)?;
    cache(&session, rebuild)?;
    let mut unity_args = vec![
        "-projectPath".into(),
        session.project.to_string_lossy().into_owned(),
    ];
    unity_args.extend(pass);
    drop(lock);
    launch(&session, &session.config.unity, unity_args, wait)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("unity-launcher: {e}");
            std::process::exit(1);
        }
    }
}
