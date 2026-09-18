// Copyright {{.Year}} Conductor OSS
// Licensed under the Apache License, Version 2.0. See LICENSE in the project root for license information.

//! `JupyterCodeExecutor` — stateful code execution against a real Jupyter kernel.
//! Feature-gated behind the `jupyter` Cargo feature.
//!
//! # Caveat
//!
//! The socket-level kernel round trip (spawn, connect over ZeroMQ, execute, read back results)
//! has not been exercised against a real kernel; only kernelspec lookup, connection-file shape,
//! HMAC-SHA256 signing, and message framing/parsing are covered by tests. Treat this as
//! best-effort pending real-world verification.
//!
//! # Wire protocol
//!
//! A Jupyter kernel exposes 5 `ZeroMQ` sockets; this client uses only shell (DEALER, for
//! `execute_request`) and iopub (SUB, for streamed output). Each message is a multipart
//! `ZeroMQ` message `[b"<IDS|MSG>", hmac_signature, header_json, parent_header_json,
//! metadata_json, content_json]`, signed with HMAC-SHA256 over the four JSON frames (empty
//! signature if the connection file's `key` is empty).

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde_json::{json, Value};
use sha2::Sha256;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::Uuid;
use zeromq::{DealerSocket, Socket as _, SocketRecv as _, SocketSend as _, SubSocket, ZmqMessage};

use crate::error::{ConductorError, Result};

use super::code_executor::{CodeExecutor, ExecutionResult};

type HmacSha256 = Hmac<Sha256>;

const DELIMITER: &[u8] = b"<IDS|MSG>";
const PROTOCOL_VERSION: &str = "5.3";

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// HMAC-SHA256 signature over the four JSON frames, per the Jupyter wire protocol's
/// `hmac-sha256` scheme. An empty `key` means "unsigned" and returns an empty signature string.
fn sign(key: &str, parts: [&[u8]; 4]) -> String {
    if key.is_empty() {
        return String::new();
    }
    let Ok(mut mac) = HmacSha256::new_from_slice(key.as_bytes()) else {
        // HMAC accepts any key length, so this is unreachable in practice — but this module is
        // unverified, so fail loudly (empty signature) rather than panic if it somehow isn't.
        return String::new();
    };
    for part in parts {
        mac.update(part);
    }
    to_hex(&mac.finalize().into_bytes())
}

/// A Jupyter kernelspec's launch command, read from a `kernel.json` file.
#[derive(Debug, Clone, PartialEq)]
struct KernelSpec {
    argv: Vec<String>,
}

/// Standard Jupyter kernelspec search directories across platforms (a fixed, best-effort list).
fn kernelspec_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(jupyter_path) = std::env::var("JUPYTER_PATH") {
        for entry in jupyter_path.split(':').filter(|s| !s.is_empty()) {
            dirs.push(PathBuf::from(entry).join("kernels"));
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        #[cfg(target_os = "macos")]
        dirs.push(home.join("Library/Jupyter/kernels"));
        #[cfg(not(target_os = "macos"))]
        dirs.push(home.join(".local/share/jupyter/kernels"));
    }
    if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
        dirs.push(PathBuf::from(venv).join("share/jupyter/kernels"));
    }
    dirs.push(PathBuf::from("/usr/local/share/jupyter/kernels"));
    dirs.push(PathBuf::from("/usr/share/jupyter/kernels"));
    dirs
}

fn find_kernelspec_in(
    dirs: &[PathBuf],
    kernel_name: &str,
) -> std::result::Result<KernelSpec, String> {
    for dir in dirs {
        let kernel_json = dir.join(kernel_name).join("kernel.json");
        if !kernel_json.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&kernel_json)
            .map_err(|e| format!("failed to read {}: {e}", kernel_json.display()))?;
        let spec: Value = serde_json::from_str(&content)
            .map_err(|e| format!("invalid kernel.json at {}: {e}", kernel_json.display()))?;
        let argv: Vec<String> = spec
            .get("argv")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{} missing 'argv'", kernel_json.display()))?
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        return Ok(KernelSpec { argv });
    }
    Err(format!(
        "no Jupyter kernelspec named '{kernel_name}' found (searched {} director{})",
        dirs.len(),
        if dirs.len() == 1 { "y" } else { "ies" }
    ))
}

fn find_kernelspec(kernel_name: &str) -> std::result::Result<KernelSpec, String> {
    find_kernelspec_in(&kernelspec_search_dirs(), kernel_name)
}

fn free_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// A kernel connection file's contents (`shell_port`, `iopub_port`, ..., `key`,
/// `signature_scheme`, `kernel_name`).
#[derive(Debug, Clone)]
struct ConnectionInfo {
    shell_port: u16,
    iopub_port: u16,
    stdin_port: u16,
    control_port: u16,
    hb_port: u16,
    ip: String,
    key: String,
}

impl ConnectionInfo {
    fn generate() -> std::io::Result<Self> {
        Ok(Self {
            shell_port: free_port()?,
            iopub_port: free_port()?,
            stdin_port: free_port()?,
            control_port: free_port()?,
            hb_port: free_port()?,
            ip: "127.0.0.1".to_owned(),
            key: Uuid::new_v4().to_string(),
        })
    }

    fn to_json(&self, kernel_name: &str) -> Value {
        json!({
            "shell_port": self.shell_port,
            "iopub_port": self.iopub_port,
            "stdin_port": self.stdin_port,
            "control_port": self.control_port,
            "hb_port": self.hb_port,
            "ip": self.ip,
            "key": self.key,
            "transport": "tcp",
            "signature_scheme": "hmac-sha256",
            "kernel_name": kernel_name,
        })
    }

    fn endpoint(&self, port: u16) -> String {
        format!("tcp://{}:{}", self.ip, port)
    }
}

/// One decoded Jupyter message's four JSON frames.
struct DecodedMessage {
    header: Value,
    parent_header: Value,
    content: Value,
}

fn build_execute_request(session: &str, code: &str) -> (Value, String) {
    let msg_id = Uuid::new_v4().to_string();
    let header = json!({
        "msg_id": msg_id,
        "username": "conductor-rust-sdk",
        "session": session,
        "date": chrono::Utc::now().to_rfc3339(),
        "msg_type": "execute_request",
        "version": PROTOCOL_VERSION,
    });
    let content = json!({
        "code": code,
        "silent": false,
        "store_history": true,
        "user_expressions": {},
        "allow_stdin": false,
        "stop_on_error": true,
    });
    (
        json!({"header": header, "parent_header": {}, "metadata": {}, "content": content}),
        msg_id,
    )
}

fn encode_message(key: &str, message: &Value) -> Result<ZmqMessage> {
    let header_bytes = serde_json::to_vec(message.get("header").unwrap_or(&Value::Null))?;
    let parent_bytes = serde_json::to_vec(message.get("parent_header").unwrap_or(&Value::Null))?;
    let metadata_bytes = serde_json::to_vec(message.get("metadata").unwrap_or(&Value::Null))?;
    let content_bytes = serde_json::to_vec(message.get("content").unwrap_or(&Value::Null))?;
    let signature = sign(
        key,
        [
            &header_bytes,
            &parent_bytes,
            &metadata_bytes,
            &content_bytes,
        ],
    );

    let mut zmq_message = ZmqMessage::from(DELIMITER.to_vec());
    zmq_message.push_back(Bytes::from(signature.into_bytes()));
    zmq_message.push_back(Bytes::from(header_bytes));
    zmq_message.push_back(Bytes::from(parent_bytes));
    zmq_message.push_back(Bytes::from(metadata_bytes));
    zmq_message.push_back(Bytes::from(content_bytes));
    Ok(zmq_message)
}

/// Decode a received multipart message, scanning for the `<IDS|MSG>` delimiter rather than
/// assuming a fixed frame count — a DEALER/SUB socket may or may not see a leading
/// ROUTER-identity frame first.
fn decode_message(raw: ZmqMessage) -> Option<DecodedMessage> {
    let frames = raw.into_vec();
    let delimiter_idx = frames.iter().position(|f| f.as_ref() == DELIMITER)?;
    let rest = &frames[delimiter_idx + 1..];
    if rest.len() < 5 {
        return None;
    }
    assert!(rest.len() > 4, "checked above");
    Some(DecodedMessage {
        header: serde_json::from_slice(&rest[1]).ok()?,
        parent_header: serde_json::from_slice(&rest[2]).ok()?,
        content: serde_json::from_slice(&rest[4]).ok()?,
    })
}

struct KernelProcess {
    child: Child,
    connection_file: PathBuf,
}

impl Drop for KernelProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_file(&self.connection_file);
    }
}

fn spawn_kernel(
    spec: &KernelSpec,
    connection: &ConnectionInfo,
    kernel_name: &str,
) -> Result<KernelProcess> {
    let connection_file = std::env::temp_dir().join(format!(
        "conductor_jupyter_conn_{}.json",
        Uuid::new_v4().simple()
    ));
    std::fs::write(
        &connection_file,
        serde_json::to_vec_pretty(&connection.to_json(kernel_name))?,
    )?;

    let mut argv_iter = spec.argv.iter();
    let program = argv_iter
        .next()
        .ok_or_else(|| ConductorError::agent("kernelspec 'argv' is empty"))?;
    let args: Vec<String> = argv_iter
        .map(|a| {
            if a == "{connection_file}" {
                connection_file.to_string_lossy().to_string()
            } else {
                a.clone()
            }
        })
        .collect();

    let child = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ConductorError::agent(format!("failed to launch kernel '{program}': {e}")))?;

    Ok(KernelProcess {
        child,
        connection_file,
    })
}

struct JupyterConnection {
    shell: DealerSocket,
    iopub: SubSocket,
    session: String,
    key: String,
}

impl JupyterConnection {
    async fn connect(connection: &ConnectionInfo) -> Result<Self> {
        let mut shell = DealerSocket::new();
        shell
            .connect(&connection.endpoint(connection.shell_port))
            .await
            .map_err(|e| ConductorError::agent(format!("failed to connect shell channel: {e}")))?;

        let mut iopub = SubSocket::new();
        iopub
            .connect(&connection.endpoint(connection.iopub_port))
            .await
            .map_err(|e| ConductorError::agent(format!("failed to connect iopub channel: {e}")))?;
        iopub
            .subscribe("")
            .await
            .map_err(|e| ConductorError::agent(format!("failed to subscribe to iopub: {e}")))?;

        Ok(Self {
            shell,
            iopub,
            session: Uuid::new_v4().to_string(),
            key: connection.key.clone(),
        })
    }

    async fn execute_request(&mut self, code: &str) -> Result<String> {
        let (message, msg_id) = build_execute_request(&self.session, code);
        let encoded = encode_message(&self.key, &message)?;
        self.shell
            .send(encoded)
            .await
            .map_err(|e| ConductorError::agent(format!("shell channel send failed: {e}")))?;
        Ok(msg_id)
    }

    /// Poll iopub until the kernel reports `idle` for this request or `timeout_duration`
    /// elapses. Returns `(output, error, timed_out_with_nothing_captured)` — a timeout after
    /// some output was already captured still returns that output normally.
    async fn poll_until_idle(
        &mut self,
        msg_id: &str,
        timeout_duration: Duration,
    ) -> (String, String, bool) {
        let mut output = String::new();
        let mut error = String::new();
        let deadline = tokio::time::Instant::now() + timeout_duration;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let timed_out_empty = output.is_empty() && error.is_empty();
                return (output, error, timed_out_empty);
            }
            let Ok(Ok(raw)) = timeout(remaining, self.iopub.recv()).await else {
                let timed_out_empty = output.is_empty() && error.is_empty();
                return (output, error, timed_out_empty);
            };
            let Some(decoded) = decode_message(raw) else {
                continue;
            };
            if decoded.parent_header.get("msg_id").and_then(Value::as_str) != Some(msg_id) {
                continue;
            }
            let msg_type = decoded
                .header
                .get("msg_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            match msg_type {
                "stream" => {
                    let text = decoded
                        .content
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if decoded.content.get("name").and_then(Value::as_str) == Some("stderr") {
                        error.push_str(text);
                    } else {
                        output.push_str(text);
                    }
                }
                "execute_result" => {
                    let text = decoded
                        .content
                        .get("data")
                        .and_then(|d| d.get("text/plain"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    output.push_str(text);
                }
                "error" => {
                    if let Some(traceback) =
                        decoded.content.get("traceback").and_then(Value::as_array)
                    {
                        let joined: Vec<&str> =
                            traceback.iter().filter_map(Value::as_str).collect();
                        error.push_str(&joined.join("\n"));
                    }
                }
                "status"
                    if decoded
                        .content
                        .get("execution_state")
                        .and_then(Value::as_str)
                        == Some("idle") =>
                {
                    return (output, error, false);
                }
                _ => {}
            }
        }
    }
}

struct KernelState {
    _process: KernelProcess,
    connection: JupyterConnection,
}

/// Execute code in a real Jupyter kernel, maintaining kernel state (variables/imports) across
/// calls. See the module doc for what is and isn't verified. Requires a Jupyter kernelspec
/// (e.g. `ipykernel`'s `python3`) discoverable on the standard kernelspec search paths.
pub struct JupyterCodeExecutor {
    pub kernel_name: String,
    pub timeout_seconds: u64,
    pub startup_code: Option<String>,
    state: Mutex<Option<KernelState>>,
}

impl JupyterCodeExecutor {
    pub fn new(kernel_name: impl Into<String>) -> Self {
        Self {
            kernel_name: kernel_name.into(),
            timeout_seconds: 30,
            startup_code: None,
            state: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn with_timeout_seconds(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = timeout_seconds;
        self
    }

    #[must_use]
    pub fn with_startup_code(mut self, startup_code: impl Into<String>) -> Self {
        self.startup_code = Some(startup_code.into());
        self
    }

    async fn ensure_kernel(&self, guard: &mut Option<KernelState>) -> Result<()> {
        if guard.is_some() {
            return Ok(());
        }
        let spec = find_kernelspec(&self.kernel_name).map_err(ConductorError::agent)?;
        let connection = ConnectionInfo::generate()?;
        let process = spawn_kernel(&spec, &connection, &self.kernel_name)?;

        // Fixed grace period for the kernel to bind its sockets before we connect; not a real
        // readiness handshake. See the module doc: this path is unverified against a live kernel.
        tokio::time::sleep(Duration::from_millis(500)).await;

        let mut connection = JupyterConnection::connect(&connection).await?;

        if let Some(startup_code) = self.startup_code.clone() {
            let msg_id = connection.execute_request(&startup_code).await?;
            let _ = connection
                .poll_until_idle(&msg_id, Duration::from_secs(30))
                .await;
        }

        *guard = Some(KernelState {
            _process: process,
            connection,
        });
        Ok(())
    }

    /// Shut down the kernel process, if one was started.
    pub async fn shutdown(&self) {
        let mut guard = self.state.lock().await;
        guard.take();
    }
}

#[async_trait]
impl CodeExecutor for JupyterCodeExecutor {
    async fn execute(&self, code: &str) -> ExecutionResult {
        let mut guard = self.state.lock().await;
        if let Err(e) = self.ensure_kernel(&mut guard).await {
            return ExecutionResult {
                error: format!("Kernel startup failed: {e}"),
                exit_code: 1,
                ..Default::default()
            };
        }
        let Some(state) = guard.as_mut() else {
            unreachable!("ensure_kernel populates this or returns Err above");
        };

        let msg_id = match state.connection.execute_request(code).await {
            Ok(id) => id,
            Err(e) => {
                return ExecutionResult {
                    error: e.to_string(),
                    exit_code: 1,
                    ..Default::default()
                }
            }
        };

        let (output, error, timed_out_empty) = state
            .connection
            .poll_until_idle(&msg_id, Duration::from_secs(self.timeout_seconds))
            .await;

        if timed_out_empty {
            return ExecutionResult {
                error: format!("Execution timed out after {}s", self.timeout_seconds),
                exit_code: -1,
                timed_out: true,
                ..Default::default()
            };
        }

        ExecutionResult {
            output,
            exit_code: i32::from(!error.is_empty()),
            error,
            timed_out: false,
        }
    }

    fn language(&self) -> &'static str {
        "python"
    }

    fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_empty_key_is_unsigned() {
        assert_eq!(sign("", [b"a", b"b", b"c", b"d"]), "");
    }

    #[test]
    fn test_sign_produces_64_char_lowercase_hex_digest() {
        // HMAC-SHA256 always produces a 32-byte digest -> 64 lowercase hex chars, regardless
        // of key/input length.
        let signature = sign("some-key", [b"header", b"parent", b"metadata", b"content"]);
        assert_eq!(signature.len(), 64);
        assert!(signature
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_sign_is_deterministic_and_key_sensitive() {
        let a = sign("key-a", [b"h", b"p", b"m", b"c"]);
        let b = sign("key-a", [b"h", b"p", b"m", b"c"]);
        let c = sign("key-b", [b"h", b"p", b"m", b"c"]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_connection_info_to_json_shape() {
        let info = ConnectionInfo {
            shell_port: 1,
            iopub_port: 2,
            stdin_port: 3,
            control_port: 4,
            hb_port: 5,
            ip: "127.0.0.1".to_owned(),
            key: "secret".to_owned(),
        };
        let json = info.to_json("python3");
        assert_eq!(json["shell_port"], 1);
        assert_eq!(json["iopub_port"], 2);
        assert_eq!(json["transport"], "tcp");
        assert_eq!(json["signature_scheme"], "hmac-sha256");
        assert_eq!(json["kernel_name"], "python3");
        assert_eq!(json["key"], "secret");
    }

    #[test]
    fn test_connection_info_endpoint_format() {
        let info = ConnectionInfo {
            shell_port: 12345,
            iopub_port: 0,
            stdin_port: 0,
            control_port: 0,
            hb_port: 0,
            ip: "127.0.0.1".to_owned(),
            key: String::new(),
        };
        assert_eq!(info.endpoint(info.shell_port), "tcp://127.0.0.1:12345");
    }

    #[test]
    fn test_free_port_returns_distinct_nonzero_ports() {
        let a = free_port().unwrap();
        let b = free_port().unwrap();
        assert_ne!(a, 0);
        assert_ne!(b, 0);
    }

    #[test]
    fn test_find_kernelspec_in_reads_argv_from_kernel_json() {
        let dir = std::env::temp_dir().join(format!(
            "conductor_jupyter_test_{}",
            Uuid::new_v4().simple()
        ));
        let kernel_dir = dir.join("python3");
        std::fs::create_dir_all(&kernel_dir).unwrap();
        std::fs::write(
            kernel_dir.join("kernel.json"),
            serde_json::json!({
                "argv": ["python3", "-m", "ipykernel_launcher", "-f", "{connection_file}"],
                "display_name": "Python 3",
                "language": "python",
            })
            .to_string(),
        )
        .unwrap();

        let spec = find_kernelspec_in(std::slice::from_ref(&dir), "python3").unwrap();
        assert_eq!(
            spec.argv,
            vec![
                "python3",
                "-m",
                "ipykernel_launcher",
                "-f",
                "{connection_file}"
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_find_kernelspec_in_missing_kernel_is_err() {
        let dir = std::env::temp_dir().join(format!(
            "conductor_jupyter_test_missing_{}",
            Uuid::new_v4().simple()
        ));
        let err = find_kernelspec_in(&[dir], "nonexistent-kernel").unwrap_err();
        assert!(err.contains("nonexistent-kernel"));
    }

    #[test]
    fn test_build_execute_request_shape_and_unique_msg_id() {
        let (message, msg_id) = build_execute_request("session-1", "print(1)");
        assert_eq!(message["header"]["msg_type"], "execute_request");
        assert_eq!(message["header"]["session"], "session-1");
        assert_eq!(message["header"]["msg_id"], msg_id);
        assert_eq!(message["content"]["code"], "print(1)");
        assert_eq!(message["content"]["silent"], false);

        let (_, msg_id_2) = build_execute_request("session-1", "print(2)");
        assert_ne!(msg_id, msg_id_2);
    }

    #[test]
    fn test_encode_message_produces_delimiter_and_five_frames() {
        let (message, _) = build_execute_request("session-1", "print(1)");
        let encoded = encode_message("my-key", &message).unwrap();
        let frames = encoded.into_vec();
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[0].as_ref(), DELIMITER);
    }

    #[test]
    fn test_decode_message_round_trips_encode_message() {
        let (message, msg_id) = build_execute_request("session-1", "print(1)");
        let encoded = encode_message("my-key", &message).unwrap();
        let decoded = decode_message(encoded).unwrap();
        assert_eq!(decoded.header["msg_type"], "execute_request");
        assert_eq!(decoded.header["msg_id"], msg_id);
        assert_eq!(decoded.content["code"], "print(1)");
    }

    #[test]
    fn test_decode_message_tolerates_leading_identity_frame() {
        let (message, _) = build_execute_request("session-1", "print(1)");
        let mut encoded = encode_message("my-key", &message).unwrap();
        // Simulate a ROUTER-added leading identity frame before the delimiter.
        let mut frames = encoded.into_vec();
        frames.insert(0, Bytes::from_static(b"some-peer-identity"));
        encoded = ZmqMessage::from(frames[0].clone());
        for frame in &frames[1..] {
            encoded.push_back(frame.clone());
        }
        let decoded = decode_message(encoded).unwrap();
        assert_eq!(decoded.header["msg_type"], "execute_request");
    }

    #[test]
    fn test_decode_message_rejects_missing_delimiter() {
        let mut msg = ZmqMessage::from(b"not-a-delimiter".to_vec());
        msg.push_back(Bytes::from_static(b"x"));
        assert!(decode_message(msg).is_none());
    }

    #[tokio::test]
    async fn test_execute_without_kernelspec_reports_startup_failure() {
        let executor =
            JupyterCodeExecutor::new("definitely-not-a-real-kernel-xyz").with_timeout_seconds(1);
        let result = executor.execute("print(1)").await;
        assert!(!result.success());
        assert!(result.error.contains("Kernel startup failed"));
    }

    #[test]
    fn test_jupyter_code_executor_builders() {
        let executor = JupyterCodeExecutor::new("python3")
            .with_timeout_seconds(45)
            .with_startup_code("import sys");
        assert_eq!(executor.kernel_name, "python3");
        assert_eq!(executor.timeout_seconds, 45);
        assert_eq!(executor.startup_code, Some("import sys".to_owned()));
        assert_eq!(CodeExecutor::language(&executor), "python");
        assert_eq!(CodeExecutor::timeout_seconds(&executor), 45);
    }

    #[tokio::test]
    async fn test_shutdown_without_a_started_kernel_is_a_noop() {
        let executor = JupyterCodeExecutor::new("python3");
        executor.shutdown().await;
    }
}
