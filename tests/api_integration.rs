//! Integration tests: spawn `serve mock` and exercise the HTTP API.

use assert_cmd::cargo::cargo_bin;
use std::net::TcpListener;
use std::process::{Child, Command};
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
    let mut cmd = Command::new(cargo_bin("agent-analyzer"));
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
fn healthz_returns_ok() {
    let port = free_port();
    let _guard = start_mock_server(port, &[]);
    wait_for_server(port);

    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{port}/api/v1/healthz")).unwrap();
    assert!(resp.status().is_success());
}

#[test]
fn state_returns_mock_targets() {
    let port = free_port();
    let _guard = start_mock_server(port, &[]);
    wait_for_server(port);

    let body: serde_json::Value =
        reqwest::blocking::get(format!("http://127.0.0.1:{port}/api/v1/state"))
            .unwrap()
            .json()
            .unwrap();

    let targets = body["targets"].as_array().unwrap();
    assert!(!targets.is_empty(), "expected mock targets in state");
}

#[test]
fn fail_flag_marks_target_failed() {
    let port = free_port();
    let _guard = start_mock_server(port, &["--fail", "model:gpt-4"]);
    wait_for_server(port);

    let body: serde_json::Value =
        reqwest::blocking::get(format!("http://127.0.0.1:{port}/api/v1/state"))
            .unwrap()
            .json()
            .unwrap();

    let failed = body["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "gpt-4")
        .expect("gpt-4 target missing");

    assert_eq!(
        failed["status"]["state"], "failed",
        "gpt-4 should be in failed state"
    );
}
