use super::{
    AdapterError,
    read_only_rpc::{ReadOnlyAppServer, TransportLimits},
};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

pub fn parse_installed_version(stdout: &[u8]) -> Result<String, AdapterError> {
    let text =
        std::str::from_utf8(stdout).map_err(|_| AdapterError("unrecognized_installed_version"))?;
    let pattern =
        regex::Regex::new(r"^codex(?:-cli)?\s+([0-9]+\.[0-9]+\.[0-9]+(?:-[a-zA-Z0-9.-]+)?)\s*$")
            .map_err(|_| AdapterError("version_check_failed"))?;
    pattern
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|s| s.as_str().to_owned())
        .ok_or(AdapterError("unrecognized_installed_version"))
}
/// Fixed command, no shell, no configurable executable, no credential or login calls.
pub async fn installed_version() -> Result<String, AdapterError> {
    let mut child = Command::new("codex")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| AdapterError("version_check_failed"))?;
    let result = timeout(Duration::from_secs(10), async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut out = child
            .stdout
            .take()
            .ok_or(AdapterError("version_check_failed"))?
            .take(1025);
        let mut err = child
            .stderr
            .take()
            .ok_or(AdapterError("version_check_failed"))?
            .take(1025);
        let (a, b) = tokio::join!(out.read_to_end(&mut stdout), err.read_to_end(&mut stderr));
        if a.is_err() || b.is_err() || stdout.len() > 1024 || stderr.len() > 1024 {
            return Err(AdapterError("version_check_failed"));
        }
        let status = child
            .wait()
            .await
            .map_err(|_| AdapterError("version_check_failed"))?;
        if !status.success() {
            return Err(AdapterError("version_check_failed"));
        }
        parse_installed_version(&stdout)
    })
    .await;
    let _ = child.start_kill();
    result.map_err(|_| AdapterError("version_check_failed"))?
}
pub async fn launch_account_reader() -> Result<(ReadOnlyAppServer, String), AdapterError> {
    let version = installed_version().await?;
    let child = Command::new("codex")
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| AdapterError("subprocess_error"))?;
    Ok((
        ReadOnlyAppServer::from_child(child, TransportLimits::default())?,
        version,
    ))
}
