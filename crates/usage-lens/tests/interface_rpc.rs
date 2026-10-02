#![cfg(unix)]
use serde_json::json;
use std::process::Stdio;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
};
use usage_lens::{
    adapters::{
        AdapterError,
        collect::{check_collection, collect_started, record_startup_failure},
        read_only_rpc::{ReadOnlyAppServer, TransportLimits},
    },
    core::UsageStore,
};
fn limits() -> TransportLimits {
    TransportLimits {
        request_timeout_ms: 500,
        lifetime_ms: 2000,
        ..TransportLimits::default()
    }
}
fn peer_child(script: &str) -> Child {
    Command::new("python3")
        .args(["-u", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}
fn peer(script: &str, limits: TransportLimits) -> ReadOnlyAppServer {
    ReadOnlyAppServer::from_child(peer_child(script), limits).unwrap()
}
const ECHO: &str = r#"import sys,json
for line in sys.stdin:
 m=json.loads(line)
 if 'id' in m:
  print(json.dumps({'id':m['id'],'result':{'method':m['method'],'params':m.get('params')}}),flush=True)
"#;
#[tokio::test]
async fn fixed_read_methods_handshake_and_close() {
    let mut client = peer(ECHO, limits());
    assert_eq!(client.read_usage().await.unwrap_err().0, "not_initialized");
    client.initialize().await.unwrap();
    assert_eq!(
        client.initialize().await.unwrap_err().0,
        "already_initialized"
    );
    let account = client.read_account().await.unwrap();
    assert_eq!(
        account,
        json!({"method":"account/read","params":{"refreshToken":false}})
    );
    assert_eq!(
        client.read_usage().await.unwrap()["method"],
        "account/usage/read"
    );
    assert_eq!(
        client.read_rate_limits().await.unwrap()["method"],
        "account/rateLimits/read"
    );
    assert!(client.diagnostics().stdout_bytes > 0);
    assert_eq!(client.diagnostics().stderr_bytes, 0);
    assert!(client.diagnostics().failure.is_none());
    client.close().await;
    assert_eq!(client.diagnostics().failure, Some("closed"));
    assert_eq!(client.read_usage().await.unwrap_err().0, "closed");
    client.close().await;
}
#[tokio::test]
async fn notifications_are_discarded_and_integer_precision_preserved() {
    let script = r#"import sys,json
for line in sys.stdin:
 m=json.loads(line)
 if 'id' in m:
  print('{"method":"raw/notification","params":{"body":"PRIVATE"}}')
  print('{"id":'+str(m['id'])+',"result":{"count":18446744073709551616123456789}}',flush=True)
"#;
    let mut client = peer(script, limits());
    client.initialize().await.unwrap();
    let value = client.read_usage().await.unwrap();
    assert_eq!(value["count"].to_string(), "18446744073709551616123456789");
    assert!(!value.to_string().contains("PRIVATE"));
    client.close().await;
}
#[tokio::test]
async fn safe_protocol_errors_and_handoff_blocks() {
    for (response, expected) in [
        ("[]", "malformed_response"),
        ("{bad", "malformed_response"),
        ("{\"id\":999,\"result\":{}}", "unexpected_response_id"),
        (
            "{\"id\":1,\"result\":{},\"error\":{}}",
            "malformed_response",
        ),
        ("{\"id\":1}", "malformed_response"),
        ("{\"method\":2}", "malformed_response"),
        (
            "{\"id\":1,\"method\":\"account/chatgptAuthTokens/refresh\",\"params\":{\"secret\":\"PRIVATE\"}}",
            "auth_handoff_required",
        ),
        (
            "{\"id\":1,\"method\":\"turn/start\"}",
            "server_request_blocked",
        ),
        (
            "{\"id\":1,\"error\":{\"code\":-32601,\"message\":\"PRIVATE\"}}",
            "unsupported_method",
        ),
        (
            "{\"id\":1,\"error\":{\"code\":999,\"message\":\"PRIVATE\"}}",
            "rpc_error",
        ),
    ] {
        let script = format!(
            "import sys,time\nsys.stdin.readline()\nprint({},flush=True)\ntime.sleep(1)",
            serde_json::to_string(response).unwrap()
        );
        let mut client = peer(&script, limits());
        let error = client.initialize().await.unwrap_err();
        assert_eq!(error.0, expected, "{response}");
        assert!(!error.to_string().contains("PRIVATE"));
        client.close().await;
    }
    let mut client = peer(
        "import sys\nsys.stdin.readline()\nsys.stdout.buffer.write(b'\\xff\\n')\nsys.stdout.flush()",
        limits(),
    );
    assert_eq!(
        client.initialize().await.unwrap_err().0,
        "malformed_response"
    );
}
#[tokio::test]
async fn subprocess_frame_stream_and_time_limits() {
    for (script, limits, expected) in [
        (
            "import time;time.sleep(2)",
            TransportLimits {
                request_timeout_ms: 20,
                ..limits()
            },
            "request_timeout",
        ),
        (
            "import time;time.sleep(2)",
            TransportLimits {
                lifetime_ms: 20,
                ..limits()
            },
            "lifetime_timeout",
        ),
        (
            "import sys;sys.stdin.readline();sys.exit(0)",
            limits(),
            "subprocess_closed",
        ),
        (
            "import sys,time;sys.stdin.readline();print('x'*200,flush=True);time.sleep(1)",
            TransportLimits {
                max_frame_bytes: 100,
                ..limits()
            },
            "frame_limit",
        ),
        (
            "import sys,time;sys.stdin.readline();sys.stdout.write('x'*200);sys.stdout.flush();time.sleep(1)",
            TransportLimits {
                max_frame_bytes: 100,
                ..limits()
            },
            "frame_limit",
        ),
        (
            "import sys,time;sys.stdin.readline();print('x'*200,flush=True);time.sleep(1)",
            TransportLimits {
                max_stdout_bytes: 100,
                ..limits()
            },
            "stdout_limit",
        ),
        (
            "import sys,time;sys.stdin.readline();sys.stderr.write('PRIVATE'*100);sys.stderr.flush();time.sleep(1)",
            TransportLimits {
                max_stderr_bytes: 100,
                ..limits()
            },
            "stderr_limit",
        ),
    ] {
        let mut client = peer(script, limits);
        assert_eq!(client.initialize().await.unwrap_err().0, expected);
        assert_eq!(client.diagnostics().failure, Some(expected));
        client.close().await;
    }
    let child = Command::new("python3")
        .args(["-c", "pass"])
        .spawn()
        .unwrap();
    assert!(matches!(
        ReadOnlyAppServer::from_child(child, limits()),
        Err(AdapterError("subprocess_error"))
    ));
    let child = Command::new("python3")
        .args(["-c", "pass"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(matches!(
        ReadOnlyAppServer::from_child(
            child,
            TransportLimits {
                max_frame_bytes: 0,
                ..limits()
            }
        ),
        Err(AdapterError("invalid_transport_limits"))
    ));
}
fn live() -> UsageStore {
    let store = UsageStore::with_clock_ms(":memory:", 1790920800000).unwrap();
    store.create_source(&json!({"id":"local","mode":"live","displayName":"Synthetic peer","provider":"codex_app_server","coverageDescription":"Fake peer only"})).unwrap();
    store
}
const CLOCK: &str = "2026-10-02T06:00:00.000Z";
fn clock() -> String {
    CLOCK.into()
}
#[tokio::test]
async fn collection_is_explicit_persists_failures_and_keeps_old_data() {
    let store = live();
    assert_eq!(
        check_collection(&store, "local", false).unwrap_err().0,
        "live_startup_opt_in_required"
    );
    assert_eq!(
        check_collection(&store, "missing", true).unwrap_err().0,
        "live_source_required"
    );
    check_collection(&store, "local", true).unwrap();
    store.ingest_observation(&json!({"sourceId":"local","method":"account/usage/read","observedAt":"2026-10-02T05:00:00.000Z","adapterVersion":"fixture","schemaBaseline":"public-docs-2026-10-02","raw":{"summary":{"lifetimeTokens":123}}})).unwrap();
    let script = r#"import sys,json
for line in sys.stdin:
 m=json.loads(line)
 if 'id' not in m: continue
 method=m['method']
 if method=='initialize': value={'id':m['id'],'result':{}}
 elif method=='account/read': value={'id':m['id'],'result':{'account':{'type':'chatgpt','planType':'pro'},'requiresOpenaiAuth':True}}
 elif method=='account/usage/read': value={'id':m['id'],'error':{'code':-32601,'message':'PRIVATE'}}
 else: value={'id':m['id'],'result':{'rateLimits':{'limitId':'codex'}}}
 print(json.dumps(value),flush=True)
"#;
    let mut client = peer(script, limits());
    let result = collect_started(&store, "local", &mut client, "0.1.0", clock)
        .await
        .unwrap();
    assert_eq!(result["outcomes"][0]["status"], "available");
    assert_eq!(result["outcomes"][1]["errorCode"], "unsupported_method");
    assert_eq!(result["outcomes"][2]["status"], "available");
    let overview = store.get_overview(&json!({"sourceId":"local"})).unwrap();
    assert_eq!(
        overview["usage"]["data"]["summary"]["lifetimeTokens"]["value"],
        "123"
    );
    assert_eq!(
        overview["usage"]["lastFailure"]["errorCode"],
        "unsupported_method"
    );
    assert!(!overview.to_string().contains("PRIVATE"));
    record_startup_failure(
        &store,
        "local",
        AdapterError("version_check_failed"),
        &clock,
    )
    .unwrap();
    store
        .update_settings(&json!({"capturePaused":true}))
        .unwrap();
    assert_eq!(
        check_collection(&store, "local", true).unwrap_err().0,
        "capture_paused"
    );
}
#[tokio::test]
async fn incompatible_accounts_do_not_trigger_other_reads_and_startup_failure_is_recorded() {
    let store = live();
    let script = r#"import sys,json
for line in sys.stdin:
 m=json.loads(line)
 if 'id' in m:
  value={} if m['method']=='initialize' else {'account':{'type':'apiKey'},'requiresOpenaiAuth':False}
  print(json.dumps({'id':m['id'],'result':value}),flush=True)
"#;
    let mut client = peer(script, limits());
    let result = collect_started(&store, "local", &mut client, "0.1.0", clock)
        .await
        .unwrap();
    assert_eq!(result["outcomes"][1]["errorCode"], "incompatible_auth_mode");
    assert_eq!(result["outcomes"][2]["errorCode"], "incompatible_auth_mode");
    let mut client = peer(
        "import sys;sys.stdin.readline();print('{bad',flush=True)",
        limits(),
    );
    assert_eq!(
        collect_started(&store, "local", &mut client, "0.1.0", clock)
            .await
            .unwrap_err()
            .0,
        "malformed_response"
    );
    let status = store.get_status().unwrap();
    assert!(!status.to_string().contains("PRIVATE"));
    let store = live();
    let mut client = peer(
        "import sys,json\nfor line in sys.stdin:\n m=json.loads(line)\n if 'id' in m: print(json.dumps({'id':m['id'],'result':None}),flush=True)",
        limits(),
    );
    let result = collect_started(&store, "local", &mut client, "0.1.0", clock)
        .await
        .unwrap();
    assert_eq!(result["outcomes"][0]["errorCode"], "invalid_payload");
}

#[tokio::test]
async fn lifetime_deadline_also_applies_between_requests() {
    let mut client = peer(
        ECHO,
        TransportLimits {
            lifetime_ms: 100,
            ..limits()
        },
    );
    client.initialize().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert_eq!(client.read_usage().await.unwrap_err().0, "lifetime_timeout");
    client.close().await;
}

#[tokio::test]
async fn peer_closed_stdin_blank_frames_and_closed_store_errors_are_safe() {
    let mut child =
        peer_child("import os,signal;os.close(0);os.write(1,b'stdin-closed\\n');signal.pause()");
    // Start the transport deadline only after the peer proves its read end is closed.
    // Reading exactly this acknowledgment leaves the RPC stream untouched. The peer
    // then waits for our explicit close/kill instead of racing a fixed sleep interval.
    let mut acknowledgment = [0u8; 13];
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        child
            .stdout
            .as_mut()
            .unwrap()
            .read_exact(&mut acknowledgment),
    )
    .await
    .expect("fake peer must acknowledge readiness within the bounded startup window")
    .expect("fake peer readiness pipe must be readable");
    assert_eq!(&acknowledgment, b"stdin-closed\n");
    let mut client = ReadOnlyAppServer::from_child(child, limits()).unwrap();
    let error = client.initialize().await.unwrap_err();
    assert_eq!(error.0, "subprocess_error");
    client.close().await;
    let mut client = peer(
        "import sys,json\nfor line in sys.stdin:\n m=json.loads(line)\n if 'id' in m: print('\\n'+json.dumps({'id':m['id'],'result':{}}),flush=True)",
        limits(),
    );
    client.initialize().await.unwrap();
    client.close().await;
    let mut store = live();
    store.close().unwrap();
    assert_eq!(
        check_collection(&store, "local", true).unwrap_err().0,
        "store_closed"
    );
    assert_eq!(
        record_startup_failure(&store, "local", AdapterError("rpc_error"), &clock)
            .unwrap_err()
            .0,
        "store_closed"
    );
    let mut client = peer(ECHO, limits());
    assert_eq!(
        collect_started(&store, "local", &mut client, "0.1.0", clock)
            .await
            .unwrap_err()
            .0,
        "store_closed"
    );
}

#[tokio::test]
async fn buffered_replies_cannot_pre_authorize_future_request_ids() {
    for second in [
        "{\"id\":2,\"result\":{\"forged\":true}}",
        "{\"id\":1,\"result\":{}}",
    ] {
        let frames = format!("{{\"id\":1,\"result\":{{}}}}\n{second}\n");
        let script = format!(
            "import os,sys,time\nsys.stdin.readline()\nos.write(1,{}.encode())\ntime.sleep(1)",
            serde_json::to_string(&frames).unwrap()
        );
        let mut client = peer(&script, limits());
        assert_eq!(
            client.initialize().await.unwrap_err().0,
            "unexpected_response_id"
        );
        client.close().await;
    }
}
