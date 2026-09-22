// Test fixture: one native parent -> a native child -> the Unity compiler command.
use launcher_core::*;
use std::process::Command;
fn run() -> Result<i32> {
    let session = Session::from_env()?;
    let bad = std::env::args().any(|a| a == "--bad");
    session.log("probe", std::env::args().collect::<Vec<_>>().join(" "));
    let status = if std::env::args().any(|a| a == "--child") {
        Command::new(&session.original_dotnet)
            .arg("exec")
            .arg(&session.original_csc)
            .args(["/noconfig", "/nostdlib", "/shared"])
            .arg(format!(
                "@{}",
                session
                    .directory
                    .join(if bad { "bad.rsp" } else { "probe.rsp" })
                    .display()
            ))
            .status()?
    } else {
        Command::new(std::env::current_exe()?)
            .arg("--child")
            .args(if bad { vec!["--bad"] } else { vec![] })
            .env_remove(SESSION_ENV) // The hook must repair an explicit custom environment.
            .status()?
    };
    Ok(status.code().unwrap_or(1))
}
fn main() {
    match run() {
        Ok(c) => std::process::exit(c),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
