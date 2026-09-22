use launcher_core::*;
use std::os::windows::process::CommandExt;
use std::{fs, path::PathBuf, process::Command};

fn run() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 || args[0] != "--session" || args[2] != "--" {
        return Err("Expected --session <file> -- <compiler arguments>".into());
    }
    let session = Session::read(&PathBuf::from(&args[1]))?;
    let cwd = std::env::current_dir()?;
    let expanded = expand_compiler_response(&args[3..], &cwd, &session.project)?;
    let prepared = prepare_args(expanded, &session.config.lang_version);
    session.log(
        "language-version",
        effective_language(&prepared, &session.config.lang_version),
    );
    let response = session
        .directory
        .join(format!("compiler-{}.rsp", std::process::id()));
    fs::write(
        &response,
        prepared
            .iter()
            .map(|a| quote(a))
            .collect::<Vec<_>>()
            .join("\n"),
    )?;
    session.log(
        "compiler",
        format!(
            "{} exec {} /noconfig @{}",
            session.config.dotnet.display(),
            session.config.csc.display(),
            response.display()
        ),
    );
    let status = Command::new(&session.config.dotnet)
        .arg("exec")
        .arg(&session.config.csc)
        .arg("/noconfig")
        .arg(format!("@{}", response.display()))
        .current_dir(cwd)
        .env(
            "DOTNET_ROOT",
            session
                .config
                .dotnet
                .parent()
                .ok_or("dotnet has no parent")?,
        )
        .env("DOTNET_MULTILEVEL_LOOKUP", "0")
        .creation_flags(0x08000000) // CREATE_NO_WINDOW; inherit Bee's stdio pipes.
        .status()?;
    let code = status.code().unwrap_or(1);
    session.log("compiler-exit", code.to_string());
    Ok(code)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("error URP0001: compiler-proxy: {e}");
            std::process::exit(1);
        }
    }
}
