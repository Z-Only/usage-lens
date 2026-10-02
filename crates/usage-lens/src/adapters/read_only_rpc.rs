//! Fixed-method, bounded newline RPC client. Provider messages and stderr are never exposed.
use super::AdapterError;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Child,
    time::{Instant, timeout, timeout_at},
};

/// Preserve integer tokens exactly and reject unsafe integral decimal/exponent spellings.
pub fn parse_lossless_json(bytes: &[u8]) -> Result<Value, AdapterError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| AdapterError("malformed_response"))?;
    fn check(value: &Value) -> Result<(), AdapterError> {
        match value {
            Value::Number(number) => {
                let text = number.to_string();
                if text.contains(['.', 'e', 'E'])
                    && number.as_f64().is_none_or(|n| {
                        !n.is_finite() || (n.fract() == 0.0 && n.abs() > 9_007_199_254_740_991.0)
                    })
                {
                    return Err(AdapterError("invalid_numeric_encoding"));
                }
            }
            Value::Array(items) => {
                for item in items {
                    check(item)?;
                }
            }
            Value::Object(items) => {
                for item in items.values() {
                    check(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    check(&value)?;
    Ok(value)
}

pub const READ_METHODS: [&str; 3] = [
    "account/read",
    "account/usage/read",
    "account/rateLimits/read",
];
#[derive(Clone, Copy, Debug)]
pub struct TransportLimits {
    pub request_timeout_ms: u64,
    pub lifetime_ms: u64,
    pub max_frame_bytes: usize,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}
impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            request_timeout_ms: 30000,
            lifetime_ms: 120000,
            max_frame_bytes: 512 * 1024,
            max_stdout_bytes: 2 * 1024 * 1024,
            max_stderr_bytes: 64 * 1024,
        }
    }
}
#[derive(Debug)]
pub struct Diagnostics {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub failure: Option<&'static str>,
}
pub struct ReadOnlyAppServer {
    child: Child,
    limits: TransportLimits,
    deadline: Instant,
    buffer: Vec<u8>,
    stdout_bytes: usize,
    stderr_bytes: usize,
    failure: Option<AdapterError>,
    initialized: bool,
    ready: bool,
    next_id: u64,
}
impl ReadOnlyAppServer {
    /// The caller supplies an already-created child with all three streams piped.
    pub fn from_child(mut child: Child, limits: TransportLimits) -> Result<Self, AdapterError> {
        if limits.request_timeout_ms == 0
            || limits.lifetime_ms == 0
            || limits.max_frame_bytes == 0
            || limits.max_stdout_bytes == 0
            || limits.max_stderr_bytes == 0
        {
            let _ = child.start_kill();
            return Err(AdapterError("invalid_transport_limits"));
        }
        if child.stdin.is_none() || child.stdout.is_none() || child.stderr.is_none() {
            let _ = child.start_kill();
            return Err(AdapterError("subprocess_error"));
        }
        Ok(Self {
            child,
            limits,
            deadline: Instant::now() + Duration::from_millis(limits.lifetime_ms),
            buffer: Vec::new(),
            stdout_bytes: 0,
            stderr_bytes: 0,
            failure: None,
            initialized: false,
            ready: false,
            next_id: 1,
        })
    }
    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            stdout_bytes: self.stdout_bytes,
            stderr_bytes: self.stderr_bytes,
            failure: self.failure.map(|e| e.0),
        }
    }
    fn fail(&mut self, code: &'static str) -> AdapterError {
        let error = *self.failure.get_or_insert(AdapterError(code));
        self.buffer.clear();
        let _ = self.child.start_kill();
        error
    }
    pub async fn initialize(&mut self) -> Result<(), AdapterError> {
        if self.initialized {
            return Err(AdapterError("already_initialized"));
        }
        self.initialized = true;
        self.request("initialize", Some(json!({"clientInfo":{"name":"usage_lens","title":"Usage Lens","version":"0.1.0"},"capabilities":null}))).await?;
        self.write(
            &json!({"method":"initialized","params":{}}),
            Instant::now() + Duration::from_millis(self.limits.request_timeout_ms),
        )
        .await?;
        self.ready = true;
        Ok(())
    }
    pub async fn read_account(&mut self) -> Result<Value, AdapterError> {
        self.read("account/read", Some(json!({"refreshToken":false})))
            .await
    }
    pub async fn read_usage(&mut self) -> Result<Value, AdapterError> {
        self.read("account/usage/read", None).await
    }
    pub async fn read_rate_limits(&mut self) -> Result<Value, AdapterError> {
        self.read("account/rateLimits/read", None).await
    }
    async fn read(&mut self, method: &str, params: Option<Value>) -> Result<Value, AdapterError> {
        if !self.ready {
            return Err(AdapterError("not_initialized"));
        }
        self.request(method, params).await
    }
    async fn write(
        &mut self,
        value: &Value,
        request_deadline: Instant,
    ) -> Result<(), AdapterError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let bytes = format!("{value}\n");
        let deadline = self.deadline.min(request_deadline);
        if Instant::now() >= deadline {
            return Err(self.fail(if self.deadline <= request_deadline {
                "lifetime_timeout"
            } else {
                "request_timeout"
            }));
        }
        let result = timeout_at(deadline, async {
            let stdin = self
                .child
                .stdin
                .as_mut()
                .ok_or(AdapterError("subprocess_error"))?;
            stdin
                .write_all(bytes.as_bytes())
                .await
                .map_err(|_| AdapterError("subprocess_error"))?;
            stdin
                .flush()
                .await
                .map_err(|_| AdapterError("subprocess_error"))
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.fail(error.0)),
            Err(_) => Err(self.fail(if self.deadline <= request_deadline {
                "lifetime_timeout"
            } else {
                "request_timeout"
            })),
        }
    }
    async fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, AdapterError> {
        if method != "initialize" && !READ_METHODS.contains(&method) {
            return Err(AdapterError("method_not_allowed"));
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({"method":method,"id":id});
        if let Some(params) = params {
            message["params"] = params;
        }
        let request_deadline =
            Instant::now() + Duration::from_millis(self.limits.request_timeout_ms);
        self.write(&message, request_deadline).await?;
        let deadline = request_deadline.min(self.deadline);
        let result = timeout_at(deadline, self.receive(id)).await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) if ["rpc_error", "unsupported_method"].contains(&error.0) => Err(error),
            Ok(Err(error)) => Err(self.fail(error.0)),
            Err(_) => Err(self.fail(if self.deadline <= request_deadline {
                "lifetime_timeout"
            } else {
                "request_timeout"
            })),
        }
    }
    async fn receive(&mut self, id: u64) -> Result<Value, AdapterError> {
        let mut stderr_open = true;
        loop {
            let mut received = None;
            while let Some(newline) = self.buffer.iter().position(|b| *b == b'\n') {
                if newline > self.limits.max_frame_bytes {
                    return Err(AdapterError("frame_limit"));
                }
                let line: Vec<u8> = self.buffer.drain(..=newline).collect();
                if newline == 0 {
                    continue;
                }
                let value = parse_lossless_json(&line[..newline])?;
                let map = value
                    .as_object()
                    .ok_or(AdapterError("malformed_response"))?;
                if let Some(method) = map.get("method") {
                    let method = method.as_str().ok_or(AdapterError("malformed_response"))?;
                    if map.contains_key("id") {
                        return Err(AdapterError(
                            if method == "account/chatgptAuthTokens/refresh" {
                                "auth_handoff_required"
                            } else {
                                "server_request_blocked"
                            },
                        ));
                    }
                    continue;
                }
                if received.is_some()
                    || map
                        .get("id")
                        .and_then(|value| {
                            crate::core::validation::integer(value, 0, 9_007_199_254_740_991).ok()
                        })
                        .and_then(|value| u64::try_from(value).ok())
                        != Some(id)
                {
                    return Err(AdapterError("unexpected_response_id"));
                }
                if map.contains_key("result") == map.contains_key("error") {
                    return Err(AdapterError("malformed_response"));
                }
                received = Some(if let Some(error) = map.get("error") {
                    Err(AdapterError(
                        if error.get("code").and_then(Value::as_i64) == Some(-32601) {
                            "unsupported_method"
                        } else {
                            "rpc_error"
                        },
                    ))
                } else {
                    Ok(value["result"].clone())
                });
            }
            if self.buffer.len() > self.limits.max_frame_bytes {
                return Err(AdapterError("frame_limit"));
            }
            if let Some(response) = received {
                return response;
            }
            let mut out = [0u8; 8192];
            let mut err = [0u8; 8192];
            tokio::select! {
                value = self.child.stdout.as_mut().ok_or(AdapterError("subprocess_error"))?.read(&mut out) => {
                    let n = value.map_err(|_|AdapterError("subprocess_error"))?;
                    if n == 0 { return Err(AdapterError("subprocess_closed")); }
                    self.stdout_bytes += n;
                    if self.stdout_bytes>self.limits.max_stdout_bytes { return Err(AdapterError("stdout_limit")); }
                    self.buffer.extend_from_slice(&out[..n]);
                }
                value = self.child.stderr.as_mut().ok_or(AdapterError("subprocess_error"))?.read(&mut err), if stderr_open => {
                    let n = value.map_err(|_|AdapterError("subprocess_error"))?;
                    stderr_open = n != 0;
                    self.stderr_bytes += n;
                    if self.stderr_bytes>self.limits.max_stderr_bytes { return Err(AdapterError("stderr_limit")); }
                }
            }
        }
    }
    pub async fn close(&mut self) {
        self.fail("closed");
        let _ = timeout(Duration::from_millis(300), self.child.wait()).await;
    }
}
impl Drop for ReadOnlyAppServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
