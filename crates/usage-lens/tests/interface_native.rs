use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Command, Stdio},
    time::Duration,
};
fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_usage-lens")
}
#[test]
fn executable_help_status_mcp_and_exit_codes() {
    let output = Command::new(binary()).arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Usage Lens")
    );
    assert!(output.stderr.is_empty());
    let output = Command::new(binary())
        .args(["status", "--demo"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["eventCount"], "56");
    assert_eq!(value["sources"][0]["mode"], "demo");
    let output = Command::new(binary()).arg("unknown").output().unwrap();
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Usage Lens: unknown_command\n"
    );
    let mut child = Command::new(binary())
        .args(["mcp", "--demo"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"usage_status","arguments":{}}}),
    ] {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 7);
    assert!(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("synthetic")
    );
}
#[cfg(unix)]
#[test]
fn native_loopback_http_lifecycle_serves_real_embedded_assets() {
    for signal in ["-TERM", "-INT"] {
        let mut child = Command::new(binary())
            .args(["serve", "--demo", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert!(line.contains("DEMO — synthetic data only"));
        let url = line.split_whitespace().last().unwrap();
        let address = url.strip_prefix("http://").unwrap();
        for path in [
            "/api/status",
            "/",
            "/usage_lens_ui.js",
            "/usage_lens_ui_bg.wasm",
        ] {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            write!(
                stream,
                "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            assert!(response.starts_with(b"HTTP/1.1 200 OK"), "{path}");
        }
        assert!(
            Command::new("kill")
                .args([signal, &child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let status = child.wait().unwrap();
        assert!(status.success());
    }
}
#[cfg(unix)]
#[test]
fn production_fixed_launcher_runs_only_hermetic_fake_codex() {
    use std::os::unix::fs::PermissionsExt;
    let python = Command::new("python3")
        .args(["-c", "import sys;print(sys.executable)"])
        .output()
        .unwrap();
    let python = String::from_utf8(python.stdout).unwrap();
    for mode in [
        "good",
        "badversion",
        "failedversion",
        "largeversion",
        "largestderr",
        "errorrpc",
        "closed",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("usage.sqlite");
        let db = db.to_str().unwrap();
        let output = Command::new(binary())
            .args(["source", "--db", db, "--source", "local", "--mode", "live"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let executable = dir.path().join("codex");
        let script = format!(
            r#"#!{}
import sys,json
mode={mode:?}
if sys.argv[1:]==['--version']:
 if mode=='failedversion': sys.exit(1)
 if mode=='badversion': print('PRIVATE_PROVIDER_TEXT');sys.exit(0)
 if mode=='largeversion': print('x'*1025);sys.exit(0)
 if mode=='largestderr': sys.stderr.write('x'*1025);sys.exit(0)
 print('codex-cli 0.99.0');sys.exit(0)
assert sys.argv[1:]==['app-server']
for line in sys.stdin:
 m=json.loads(line)
 if 'id' not in m: continue
 if mode=='closed': sys.exit(0)
 if mode=='errorrpc': print(json.dumps({{'id':m['id'],'error':{{'code':999,'message':'PRIVATE_PROVIDER_TEXT'}}}}),flush=True);continue
 assert m['method'] in ['initialize','account/read','account/usage/read','account/rateLimits/read']
 if m['method']=='account/read':
  assert m['params']=={{'refreshToken':False}}
  result={{'account':{{'type':'chatgpt','planType':'pro'}},'requiresOpenaiAuth':True}}
 elif m['method']=='account/usage/read': result={{'summary':{{'lifetimeTokens':123456789012345678901}}}}
 elif m['method']=='account/rateLimits/read': result={{'rateLimits':{{'limitId':'codex'}}}}
 else: result={{}}
 print(json.dumps({{'id':m['id'],'result':result}}),flush=True)
"#,
            python.trim()
        );
        std::fs::write(&executable, script).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        // This PATH contains only our test fixture. A real installed Codex cannot be selected.
        let output = Command::new(binary())
            .env("PATH", dir.path())
            .args([
                "collect",
                "--db",
                db,
                "--source",
                "local",
                "--accept-startup-risk",
            ])
            .output()
            .unwrap();
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!all.contains("PRIVATE_PROVIDER_TEXT"));
        assert_eq!(output.status.success(), mode == "good", "{mode}: {all}");
        if mode == "good" {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["outcomes"][2]["status"], "available");
        }
        let overview = Command::new(binary())
            .args(["overview", "--db", db, "--source", "local"])
            .output()
            .unwrap();
        assert!(overview.status.success());
        let value: Value = serde_json::from_slice(&overview.stdout).unwrap();
        if mode != "good" {
            assert!(value["account"]["lastFailure"].is_object());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage.sqlite");
    let db = db.to_str().unwrap();
    assert!(
        Command::new(binary())
            .args(["source", "--db", db, "--source", "local", "--mode", "live"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let output = Command::new(binary())
        .env("PATH", dir.path())
        .args([
            "collect",
            "--db",
            db,
            "--source",
            "local",
            "--accept-startup-risk",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("version_check_failed")
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_cli_argument_is_sanitized_without_panic() {
    use std::os::unix::ffi::OsStringExt;
    let output = Command::new(binary())
        .arg(std::ffi::OsString::from_vec(vec![255]))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.stderr, b"Usage Lens: invalid_argument\n");
}

#[cfg(unix)]
#[test]
fn partial_http_mutation_body_times_out_with_safe_security_headers() {
    let mut child = Command::new(binary())
        .args(["serve", "--demo", "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let url = line.split_whitespace().last().unwrap();
    let address = url.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    write!(stream,"POST /api/settings HTTP/1.1\r\nHost: {address}\r\nOrigin: {url}\r\nContent-Type: application/json\r\nX-Usage-Lens-Request: local-ui\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{{").unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let response = String::from_utf8_lossy(&bytes);
    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "{response}"
    );
    assert!(response.contains("request_timeout"));
    assert!(response.to_lowercase().contains("x-frame-options: deny"));
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(child.wait().unwrap().success());
}
