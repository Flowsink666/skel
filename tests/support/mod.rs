// Each integration-test binary uses a different subset of the shared helpers.
#![allow(dead_code)]

pub mod mcp;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Fixture(pub PathBuf);

impl Fixture {
    pub fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "skel-test-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => panic!("Cannot create fixture {}: {err}", path.display()),
            }
        }
    }

    pub fn write(&self, name: &str, source: &str) -> PathBuf {
        self.write_bytes(name, source.as_bytes())
    }

    pub fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let relative = Path::new(name);
        assert!(relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_))));
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    /// Include empty directories as well as file bytes to detect unexpected writes.
    pub fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut snapshot = BTreeMap::new();
        let mut directories = vec![self.0.clone()];
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let relative = path.strip_prefix(&self.0).unwrap().to_path_buf();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() {
                    snapshot.insert(relative, None);
                    directories.push(path);
                } else if kind.is_file() {
                    snapshot.insert(relative, Some(fs::read(path).unwrap()));
                } else {
                    panic!("Snapshot does not follow symlinks: {}", path.display());
                }
            }
        }
        snapshot
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Delete only the exact, directly-created child of the system temp directory.
        let Some(name) = self.0.file_name() else {
            return;
        };
        if !name.to_string_lossy().starts_with("skel-test-") {
            return;
        }
        if let (Ok(path), Ok(temp)) = (self.0.canonicalize(), std::env::temp_dir().canonicalize()) {
            // A fixture root replaced with a symlink must never redirect cleanup.
            if path == temp.join(name) {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

/// Reap subprocesses even if an assertion or a protocol read panics.
pub struct ChildGuard(pub Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn cli_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_skel"))
}

pub fn run_cli(root: &Path, arguments: &[&str]) -> Output {
    let mut command = cli_command();
    command.arg(root).args(arguments);
    run_command(command)
}

/// Drain both pipes concurrently and bound the entire process lifetime.
pub fn run_command(mut command: Command) -> Output {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let description = format!("{command:?}");
    let mut child = ChildGuard(command.spawn().unwrap());
    let mut stdout = child.0.stdout.take().unwrap();
    let mut stderr = child.0.stderr.take().unwrap();
    let out_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let err_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "Process timed out: {description}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: out_reader.join().unwrap(),
        stderr: err_reader.join().unwrap(),
    }
}

pub fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

pub fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap()
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout: {}\nstderr: {}",
        output.status,
        stdout(output),
        stderr(output)
    );
}
