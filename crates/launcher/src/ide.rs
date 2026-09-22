use launcher_core::*;
use std::{fs, path::Path};

const SOURCE: &str = include_str!("../../../unity/RoslynLauncherProjectSync.cs");
const ASMDEF: &str = include_str!("../../../unity/UnityRoslynLauncher.Editor.asmdef");
const OWNER: &str = "UnityRoslynLauncher managed IDE bridge v1\n";

pub fn install(session: &Session) -> Result<()> {
    if !session.config.sync_ide {
        return Ok(());
    }
    let root = session.project.join("Assets/__UnityRoslynLauncher");
    let marker = root.join("launcher-owned.txt");
    if root.exists() && fs::read_to_string(&marker).ok().as_deref() != Some(OWNER) {
        return Err(format!(
            "Refusing to overwrite unowned IDE bridge directory: {}",
            root.display()
        )
        .into());
    }
    // Do not follow a project-provided junction into unrelated directories.
    for path in [&root, &root.join("Editor")] {
        if let Ok(meta) = fs::symlink_metadata(path) {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err("IDE bridge directory must not be a reparse point".into());
            }
        }
    }
    fs::create_dir_all(root.join("Editor"))?;
    write_changed(&marker, OWNER)?;
    write_changed(&root.join("Editor/RoslynLauncherProjectSync.cs"), SOURCE)?;
    write_changed(
        &root.join("Editor/UnityRoslynLauncher.Editor.asmdef"),
        ASMDEF,
    )?;
    session.log(
        "ide-bridge",
        format!("{} (C# {})", root.display(), session.config.lang_version),
    );
    Ok(())
}

fn write_changed(path: &Path, content: &str) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("IDE bridge file must not be a reparse point".into());
        }
    }
    if fs::read(path).ok().as_deref() != Some(content.as_bytes()) {
        fs::write(path, content)?;
    }
    Ok(())
}
