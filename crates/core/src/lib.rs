use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const SESSION_ENV: &str = "UNITY_ROSLYN_SESSION";
static LOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub unity: PathBuf,
    pub dotnet: PathBuf,
    pub csc: PathBuf,
    pub lang_version: String,
    #[serde(default = "default_sync_ide")]
    pub sync_ide: bool,
}

fn default_sync_ide() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub config: Config,
    pub project: PathBuf,
    pub hook: PathBuf,
    pub proxy: PathBuf,
    pub directory: PathBuf,
    pub original_csc: PathBuf,
    pub original_dotnet: PathBuf,
    pub bee: PathBuf,
    pub extra_propagate: Vec<PathBuf>,
}

impl Session {
    pub fn read(path: &Path) -> Result<Self> {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    pub fn from_env() -> Result<Self> {
        Self::read(&PathBuf::from(
            std::env::var_os(SESSION_ENV).ok_or("Missing launcher session")?,
        ))
    }
    pub fn log(&self, event: &str, detail: impl AsRef<str>) {
        let Ok(_guard) = LOG_LOCK.lock() else {
            return;
        };
        let line = serde_json::json!({"pid": std::process::id(), "event": event, "detail": detail.as_ref()});
        // Serialize threads too: Bee launches many jobs concurrently.
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.directory.join(format!("{}.jsonl", std::process::id())))
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

pub fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

// Windows CRT quoting; also accepted by Roslyn's response-file tokenizer.
pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in s.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
        } else {
            out.push_str(&"\\".repeat(slashes));
        }
        slashes = 0;
        out.push(c);
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}
pub fn command(args: &[String]) -> String {
    args.iter().map(|x| quote(x)).collect::<Vec<_>>().join(" ")
}

pub fn split_args(s: &str, response: bool) -> Result<Vec<String>> {
    let chars: Vec<char> = s.trim_start_matches('\u{feff}').chars().collect();
    let (mut i, mut out) = (0, Vec::new());
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        if response && chars[i] == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        let (mut token, mut quoted) = (String::new(), false);
        while i < chars.len() && (quoted || !chars[i].is_whitespace()) {
            let mut slash = 0;
            while i < chars.len() && chars[i] == '\\' {
                slash += 1;
                i += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                token.push_str(&"\\".repeat(slash / 2));
                if slash % 2 == 1 {
                    token.push('"');
                } else if quoted && i + 1 < chars.len() && chars[i + 1] == '"' {
                    token.push('"');
                    i += 1;
                } else {
                    quoted = !quoted;
                }
                i += 1;
            } else {
                token.push_str(&"\\".repeat(slash));
                if i < chars.len() && (quoted || !chars[i].is_whitespace()) {
                    token.push(chars[i]);
                    i += 1;
                }
            }
        }
        if quoted {
            return Err("Unclosed quote in compiler arguments".into());
        }
        out.push(token);
    }
    Ok(out)
}

pub fn same_path(a: impl AsRef<Path>, b: impl AsRef<Path>) -> bool {
    fn norm(p: &Path) -> String {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_start_matches("\\\\?\\")
            .to_lowercase()
    }
    norm(a.as_ref()) == norm(b.as_ref())
}

pub fn compiler_args(args: &[String], session: &Session) -> Option<Vec<String>> {
    if args.len() >= 3
        && same_path(&args[0], &session.original_dotnet)
        && args[1].eq_ignore_ascii_case("exec")
        && same_path(&args[2], &session.original_csc)
    {
        Some(args[3..].to_vec())
    } else {
        None
    }
}

fn read_response(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        if bytes.len() % 2 != 0 {
            return Err("Odd UTF-16 response file length".into());
        }
        let little = bytes[0] == 0xff;
        let words: Vec<_> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| {
                if little {
                    u16::from_le_bytes([b[0], b[1]])
                } else {
                    u16::from_be_bytes([b[0], b[1]])
                }
            })
            .collect();
        Ok(String::from_utf16(&words)?)
    } else {
        Ok(String::from_utf8(bytes)?)
    }
}

pub fn expand_response(args: &[String], cwd: &Path, depth: usize) -> Result<Vec<String>> {
    expand_response_inner(args, cwd, depth, None)
}

// Unity 2022.3 emits its built-in language setting first in the primary Bee
// response file, then emits user compiler options. Only that generated setting
// is replaced. An explicit user setting of 9.0 must remain an override.
pub fn expand_compiler_response(
    args: &[String],
    cwd: &Path,
    project: &Path,
) -> Result<Vec<String>> {
    expand_response_inner(args, cwd, 0, Some(project))
}

fn is_unity_response(path: &Path, project: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let base = project
        .join("Library/Bee/artifacts")
        .to_string_lossy()
        .replace('/', "\\")
        .to_lowercase();
    normalized.starts_with(&(base + "\\"))
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rsp"))
        && path
            .parent()
            .and_then(Path::extension)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dag"))
}

pub fn language_option(argument: &str) -> Option<&str> {
    let body = argument
        .strip_prefix('-')
        .or_else(|| argument.strip_prefix('/'))?;
    let (key, value) = body.split_once(':')?;
    key.eq_ignore_ascii_case("langversion").then_some(value)
}

pub fn effective_language<'a>(args: &'a [String], default: &'a str) -> &'a str {
    args.iter()
        .filter_map(|arg| language_option(arg))
        .next_back()
        .unwrap_or(default)
}

fn expand_response_inner(
    args: &[String],
    cwd: &Path,
    depth: usize,
    unity_project: Option<&Path>,
) -> Result<Vec<String>> {
    if depth > 16 {
        return Err("Response files nested too deeply / cyclic response file".into());
    }
    let mut result = Vec::new();
    for arg in args {
        if let Some(file) = arg.strip_prefix('@') {
            let path = cwd.join(file);
            let contents = read_response(&path)?;
            let mut tokens = split_args(&contents, true)?;
            // Only direct primary Bee inputs, never nested user @response files.
            if depth == 0 && unity_project.is_some_and(|p| is_unity_response(&path, p)) {
                if let Some(index) = tokens.iter().position(|t| language_option(t).is_some()) {
                    tokens.remove(index);
                }
            }
            result.extend(expand_response_inner(
                &tokens,
                cwd,
                depth + 1,
                unity_project,
            )?);
        } else {
            result.push(arg.clone());
        }
    }
    Ok(result)
}

pub fn prepare_args(args: Vec<String>, version: &str) -> Vec<String> {
    let mut args: Vec<_> = args
        .into_iter()
        .filter(|a| {
            let a = a.to_ascii_lowercase();
            let a = a.trim_start_matches(['/', '-']);
            !(a == "shared"
                || a == "shared+"
                || a == "shared-"
                || a.starts_with("shared:")
                || a == "noconfig")
        })
        .collect();
    // Roslyn uses the last language option: prepend the fallback.
    args.insert(0, format!("/langversion:{version}"));
    args
}

/// Route package assemblies without a package-owned csc.rsp through Unity's
/// bundled compiler. Assets/csc.rsp is project-wide and is not a package opt-in.
pub fn is_unconfigured_package(args: &[String], project: &Path) -> bool {
    let mut package_root: Option<PathBuf> = None;
    let mut sources = Vec::new();
    for argument in args {
        if argument.starts_with('-') || argument.starts_with('/')
            || !argument.to_ascii_lowercase().ends_with(".cs") {
            continue;
        }
        let normalized = argument.replace('\\', "/");
        let project_prefix = format!("{}/", project.to_string_lossy().replace('\\', "/"));
        let relative = if normalized.to_ascii_lowercase().starts_with(&project_prefix.to_ascii_lowercase()) {
            &normalized[project_prefix.len()..]
        } else {
            normalized.trim_start_matches("./")
        };
        let parts: Vec<_> = relative.split('/').collect();
        let root = if parts.len() >= 4
            && parts[0].eq_ignore_ascii_case("Library")
            && parts[1].eq_ignore_ascii_case("PackageCache")
        {
            Some(project.join("Library/PackageCache").join(parts[2]))
        } else if parts.len() >= 3 && parts[0].eq_ignore_ascii_case("Packages") {
            Some(project.join("Packages").join(parts[1]))
        } else {
            None
        };
        if let Some(root) = root {
            if package_root.as_ref().is_some_and(|prior| prior != &root) {
                return false;
            }
            package_root = Some(root);
            sources.push(project.join(&relative));
        } else if !relative.to_ascii_lowercase().starts_with("library/bee/") {
            // Do not reroute a user assembly that mixes package and Assets sources.
            return false;
        }
    }
    let Some(root) = package_root else { return false };
    for source in sources {
        let mut parent = source.parent();
        while let Some(directory) = parent {
            if directory.join("csc.rsp").is_file() {
                return false;
            }
            if directory == root { break; }
            parent = directory.parent();
        }
    }
    true
}

pub fn without_define(args: Vec<String>, symbol: &str) -> Vec<String> {
    args.into_iter().filter_map(|argument| {
        let body = argument.trim_start_matches(['-', '/']);
        let Some((key, value)) = body.split_once(':') else { return Some(argument) };
        if !key.eq_ignore_ascii_case("define") && !key.eq_ignore_ascii_case("d") {
            return Some(argument);
        }
        let kept: Vec<_> = value.split([';', ','])
            .filter(|part| !part.eq_ignore_ascii_case(symbol) && !part.is_empty())
            .collect();
        (!kept.is_empty()).then(|| format!("/define:{}", kept.join(";")))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unconfigured_package_uses_unity_but_package_response_opts_in() {
        let project = std::env::temp_dir().join(format!("roslyn-package-test-{}", std::process::id()));
        let package = project.join("Library/PackageCache/com.example@1.0.0/Runtime");
        fs::create_dir_all(&package).unwrap();
        let args = vec![
            "-out:Library/Bee/artifacts/Example.dll".into(),
            "Library/PackageCache/com.example@1.0.0/Runtime/Example.cs".into(),
            "-langversion:preview".into(),
        ];
        assert!(is_unconfigured_package(&args, &project));
        assert!(!is_unconfigured_package(&["Assets/Scripts/Game.cs".into()], &project));
        assert!(!is_unconfigured_package(&[args[1].clone(), "Assets/Game.cs".into()], &project));
        let absolute = vec![package.join("Example.cs").to_string_lossy().to_string()];
        assert!(is_unconfigured_package(&absolute, &project));
        fs::write(package.join("csc.rsp"), "-langversion:14").unwrap();
        assert!(!is_unconfigured_package(&args, &project));
        fs::remove_dir_all(project).unwrap();
    }
    #[test]
    fn package_drops_advance_define_without_dropping_unity_defines() {
        assert_eq!(without_define(vec!["-define:UNITY_EDITOR;MIRA_ADVANCE;DEBUG".into(),
            "/nullable:enable".into()], "MIRA_ADVANCE"),
            ["/define:UNITY_EDITOR;DEBUG", "/nullable:enable"]);
        assert!(without_define(vec!["/d:MIRA_ADVANCE".into()], "MIRA_ADVANCE").is_empty());
    }
    #[test]
    fn quoting_round_trip() {
        let input: Vec<String> = [
            "",
            "plain",
            "a b",
            "中文路径\\",
            "a\\\"b",
            "x\"y",
            "C:\\path with space\\",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(split_args(&command(&input), false).unwrap(), input);
    }
    #[test]
    fn response_comments_and_defines() {
        assert_eq!(
            split_args("# comment\n/r:\"a b.dll\"\n/define:X;Y\n\"a#b.cs\"", true).unwrap(),
            ["/r:a b.dll", "/define:X;Y", "a#b.cs"]
        );
    }
    #[test]
    fn explicit_language_overrides_default_and_server_is_disabled() {
        let a = [
            "-langversion:9.0",
            "/shared",
            "-shared:pipe",
            "-noconfig",
            "/r:UnityEngine.dll",
            "a.cs",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            prepare_args(a, "12"),
            [
                "/langversion:12",
                "-langversion:9.0",
                "/r:UnityEngine.dll",
                "a.cs"
            ]
        );
    }
    #[test]
    fn rejects_broken_quotes() {
        assert!(split_args("\"broken", true).is_err());
    }

    #[test]
    fn bee_default_is_distinct_from_explicit_user_overrides() {
        let project = std::env::temp_dir().join(format!("roslyn-bee-test-{}", std::process::id()));
        let dag = project.join("Library/Bee/artifacts/test.dag");
        fs::create_dir_all(&dag).unwrap();
        let response = dag.join("Example.rsp");
        let input = vec![format!("@{}", response.display())];
        for (user, expected) in [
            ("", "12"),
            ("/langversion:14", "14"),
            ("-langversion:9.0", "9.0"),
            ("/langversion:14 /langversion:10", "10"),
            ("/LANGVERSION:preview", "preview"),
        ] {
            fs::write(
                &response,
                format!("-langversion:9.0 /define:KEEP_ME {user}"),
            )
            .unwrap();
            let prepared = prepare_args(
                expand_compiler_response(&input, &project, &project).unwrap(),
                "12",
            );
            assert_eq!(effective_language(&prepared, "12"), expected);
            assert!(prepared.contains(&"/define:KEEP_ME".to_string()));
        }
        let custom = project.join("custom.rsp");
        fs::write(&custom, "-langversion:14 /nullable:enable").unwrap();
        fs::write(&response, "-langversion:9.0 @custom.rsp").unwrap();
        let prepared = prepare_args(
            expand_compiler_response(&input, &project, &project).unwrap(),
            "12",
        );
        assert_eq!(effective_language(&prepared, "12"), "14");
        assert!(prepared.contains(&"/nullable:enable".to_string()));
        // A directly supplied custom response has no generated default to strip.
        let prepared = prepare_args(
            expand_compiler_response(&["@custom.rsp".into()], &project, &project).unwrap(),
            "12",
        );
        assert_eq!(effective_language(&prepared, "12"), "14");
        fs::remove_file(custom).unwrap();
        fs::remove_file(response).unwrap();
        for directory in [
            dag,
            project.join("Library/Bee/artifacts"),
            project.join("Library/Bee"),
            project.join("Library"),
            project,
        ] {
            fs::remove_dir(directory).unwrap();
        }
    }

    #[test]
    fn nested_unicode_response_and_cycle() {
        let root = std::env::temp_dir().join(format!("roslyn-rsp-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let utf16: Vec<u8> = "\u{feff}/r:\"中文 space.dll\"\n-langversion:9.0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        fs::write(root.join("inner.rsp"), utf16).unwrap();
        fs::write(
            root.join("outer.rsp"),
            "# comment\n@inner.rsp\n\"code file.cs\"",
        )
        .unwrap();
        let args = expand_response(&["@outer.rsp".into()], &root, 0).unwrap();
        assert_eq!(
            prepare_args(args, "12"),
            [
                "/langversion:12",
                "/r:中文 space.dll",
                "-langversion:9.0",
                "code file.cs"
            ]
        );
        fs::write(root.join("outer.rsp"), "@outer.rsp").unwrap();
        assert!(expand_response(&["@outer.rsp".into()], &root, 0).is_err());
        for file in ["inner.rsp", "outer.rsp"] {
            fs::remove_file(root.join(file)).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }
}
