//! Integration tests: spawn `serve mock` then run `status` against it.

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::net::TcpListener;
use std::process::{Child, Command as StdCommand};
use std::thread;
use std::time::Duration;

struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_mock_server(port: u16, extra_args: &[&str]) -> ServerGuard {
    let bind = format!("127.0.0.1:{port}");
    let mut cmd = StdCommand::new(cargo_bin("agent-analyzer"));
    cmd.args(["serve", "--bind", &bind, "mock"]);
    cmd.args(extra_args);
    ServerGuard(cmd.spawn().expect("failed to spawn agent-analyzer"))
}

fn wait_for_server(port: u16) {
    for _ in 0..50 {
        if reqwest::blocking::get(format!("http://127.0.0.1:{port}/api/v1/healthz")).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(200));
    }
    panic!("server on port {port} never became ready");
}

#[test]
fn status_text_shows_targets() {
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let _guard = start_mock_server(port, &[]);
    wait_for_server(port);

    Command::cargo_bin("agent-analyzer")
        .unwrap()
        .args(["status", "--url", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("gpt-4"))
        .stdout(predicate::str::contains("✓"));
}

#[test]
fn status_json_is_valid() {
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let _guard = start_mock_server(port, &[]);
    wait_for_server(port);

    let output = Command::cargo_bin("agent-analyzer")
        .unwrap()
        .args(["status", "--url", &url, "--format", "json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("status --format json output is not valid JSON");
    assert!(parsed.is_array(), "expected a JSON array");
}

#[test]
fn status_shows_failed_when_flagged() {
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let _guard = start_mock_server(port, &["--fail", "model:gpt-4"]);
    wait_for_server(port);

    Command::cargo_bin("agent-analyzer")
        .unwrap()
        .args(["status", "--url", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("✗"));
}
