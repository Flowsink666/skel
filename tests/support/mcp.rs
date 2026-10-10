use super::{cli_command, ChildGuard, Fixture, PROCESS_TIMEOUT};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{ChildStdin, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub struct Client {
    child: ChildGuard,
    input: Option<ChildStdin>,
    messages: Receiver<Result<Value, String>>,
    pending: BTreeMap<u64, Value>,
    next_id: u64,
    readers: Vec<JoinHandle<()>>,
    stderr: Arc<Mutex<String>>,
    pub initialization: Value,
}

impl Client {
    pub fn new(fixture: &Fixture) -> Self {
        Self::with_root(&fixture.0)
    }

    pub fn with_root(root: &Path) -> Self {
        let mut command = cli_command();
        command
            .arg(root)
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ChildGuard(command.spawn().unwrap());
        let input = child.0.stdin.take().unwrap();
        let stdout = child.0.stdout.take().unwrap();
        let stderr_pipe = child.0.stderr.take().unwrap();
        let stderr = Arc::new(Mutex::new(String::new()));
        let errors = Arc::clone(&stderr);
        let err_reader = thread::spawn(move || {
            for line in BufReader::new(stderr_pipe).lines() {
                let mut errors = errors.lock().unwrap();
                match line {
                    Ok(line) => {
                        errors.push_str(&line);
                        errors.push('\n');
                    }
                    Err(err) => {
                        errors.push_str(&err.to_string());
                        break;
                    }
                }
            }
        });
        let (sender, messages) = mpsc::channel();
        let out_reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let message = line.map_err(|err| err.to_string()).and_then(|line| {
                    serde_json::from_str(&line)
                        .map_err(|err| format!("Non-protocol stdout {line:?}: {err}"))
                });
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input: Some(input),
            messages,
            pending: BTreeMap::new(),
            next_id: 1,
            readers: vec![out_reader, err_reader],
            stderr,
            initialization: Value::Null,
        };
        let response = client.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "skel-integration-test", "version": "1.0"}
            }),
        );
        assert_eq!(response["result"]["serverInfo"]["name"], "skel");
        assert!(response["result"]["capabilities"]["tools"].is_object());
        client.initialization = response["result"].clone();
        client.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        client
    }

    pub fn send(&mut self, message: Value) {
        let input = self.input.as_mut().expect("MCP stdin is closed");
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    pub fn send_request(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    pub fn receive(&mut self, id: u64) -> Value {
        if let Some(message) = self.pending.remove(&id) {
            return message;
        }
        // Notifications do not reset the request deadline. Responses may arrive out of order.
        let deadline = Instant::now() + PROCESS_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = self
                .messages
                .recv_timeout(remaining)
                .unwrap_or_else(|err| {
                    panic!(
                        "MCP response {id} failed: {err}; stderr: {}",
                        self.stderr.lock().unwrap()
                    )
                })
                .unwrap_or_else(|err| panic!("{err}; stderr: {}", self.stderr.lock().unwrap()));
            assert_eq!(message["jsonrpc"], "2.0", "{message}");
            if let Some(response_id) = message.get("id").and_then(Value::as_u64) {
                assert!(
                    message.get("result").is_some() ^ message.get("error").is_some(),
                    "{message}"
                );
                if response_id == id {
                    return message;
                }
                assert!(
                    response_id < self.next_id,
                    "Unrequested response: {message}"
                );
                assert!(
                    self.pending.insert(response_id, message).is_none(),
                    "Duplicate response ID {response_id}"
                );
            } else {
                assert!(
                    message["method"].is_string(),
                    "Unexpected stdout: {message}"
                );
                assert!(
                    message.get("id").is_none(),
                    "Unexpected server request: {message}"
                );
            }
        }
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.send_request(method, params);
        self.receive(id)
    }

    pub fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    pub fn close(&mut self) {
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.0.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "MCP exited with {status}; stderr: {}",
                    self.stderr.lock().unwrap()
                );
                break;
            }
            assert!(Instant::now() < deadline, "MCP did not exit on EOF");
            thread::sleep(Duration::from_millis(10));
        }
        self.join_readers();
        assert!(
            self.pending.is_empty(),
            "Unconsumed MCP responses: {:?}",
            self.pending
        );
        for message in self.messages.try_iter() {
            let message = message.unwrap();
            assert_eq!(message["jsonrpc"], "2.0");
            assert!(
                message["method"].is_string() && message.get("id").is_none(),
                "Unexpected response at shutdown: {message}"
            );
        }
        for line in self.stderr.lock().unwrap().lines() {
            let summary = line
                .strip_prefix("skel context summary: ")
                .unwrap_or_else(|| panic!("Unexpected MCP stderr: {line}"));
            let metadata: Value = serde_json::from_str(summary).unwrap();
            assert!(metadata["stats"].is_object(), "{line}");
            assert!(metadata["files"].is_array(), "{line}");
            assert!(metadata["warnings"].is_array(), "{line}");
        }
    }

    pub fn logs(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.0.kill();
        let _ = self.child.0.wait();
        self.join_readers();
    }
}

pub fn success(response: &Value) -> &Value {
    assert!(response.get("error").is_none(), "{response}");
    let result = &response["result"];
    assert!(result.is_object(), "{response}");
    assert_ne!(result["isError"], true, "{response}");
    assert!(result["structuredContent"].is_object(), "{response}");
    &result["structuredContent"]
}

pub fn markdown(response: &Value) -> &str {
    success(response);
    let content = response["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");
    content[0]["text"].as_str().unwrap()
}

pub fn tool_error<'a>(response: &'a Value, fragment: &str) -> &'a str {
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["isError"], true, "{response}");
    assert!(
        response["result"].get("structuredContent").is_none(),
        "{response}"
    );
    let content = response["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");
    let message = content[0]["text"].as_str().unwrap();
    assert!(
        message.contains(fragment),
        "Expected {fragment:?}: {response}"
    );
    message
}
