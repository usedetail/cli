//! Integration tests for the Detail CLI.
//!
//! These tests run the compiled `detail` binary against the live Detail API.
//! Set `DETAIL_API_KEY` in the environment to enable them; when absent the
//! tests are silently skipped.

use std::path::PathBuf;
use std::process::Command;

const REPO: &str = "usedetail/cli";

// ── Helpers ─────────────────────────────────────────────────────────

fn api_key() -> Option<String> {
    std::env::var("DETAIL_API_KEY").ok()
}

/// Return the key or skip the calling test.
macro_rules! require_api_key {
    () => {
        match api_key() {
            Some(k) => k,
            None => {
                eprintln!("DETAIL_API_KEY not set — skipping");
                return;
            }
        }
    };
}

/// A disposable, isolated CLI environment backed by a temp config directory.
struct Env {
    config_dir: PathBuf,
}

struct Output {
    success: bool,
    stdout: String,
    stderr: String,
}

impl Env {
    /// Create a fresh config dir without logging in.
    fn new(name: &str) -> Self {
        let config_dir =
            std::env::temp_dir().join(format!("detail-integ-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&config_dir);
        std::fs::create_dir_all(&config_dir).unwrap();
        Self { config_dir }
    }

    /// Create a fresh config dir and authenticate with the given key.
    fn authenticated(api_key: &str, name: &str) -> Self {
        let env = Self::new(name);
        let out = env.run(&["auth", "login", "--token", api_key]);
        assert!(out.success, "auth login failed: {}", out.stderr);
        assert!(
            out.stdout.contains("Successfully authenticated"),
            "unexpected login output: {}",
            out.stdout
        );
        env
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_detail"));
        cmd.env("XDG_CONFIG_HOME", &self.config_dir);
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = self.cmd().args(args).output().expect("failed to execute detail binary");
        Output {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn run_json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.run(args);
        assert!(
            out.success,
            "`detail {}` failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            out.stdout,
            out.stderr,
        );
        serde_json::from_str(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "invalid JSON from `detail {}`:\n{e}\nstdout: {}",
                args.join(" "),
                out.stdout,
            )
        })
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.config_dir);
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn version() {
    let env = Env::new("version");
    let out = env.run(&["version"]);
    assert!(out.success, "version failed: {}", out.stderr);
    assert!(
        out.stdout.contains("detail-cli v"),
        "unexpected version output: {}",
        out.stdout,
    );
}

#[test]
fn auth_login_and_status() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "auth_login_and_status");

    let out = env.run(&["auth", "status"]);
    assert!(out.success, "auth status failed: {}", out.stderr);
    assert!(
        out.stdout.contains("Authenticated"),
        "expected Authenticated in: {}",
        out.stdout,
    );
}

#[test]
fn auth_logout() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "auth_logout");

    let out = env.run(&["auth", "logout"]);
    assert!(out.success, "auth logout failed: {}", out.stderr);
    assert!(
        out.stdout.contains("Logged out"),
        "expected Logged out in: {}",
        out.stdout,
    );

    // Commands should fail after logout
    let out = env.run(&["repos", "list", "--format", "json"]);
    assert!(
        !out.success,
        "repos list should fail after logout but succeeded",
    );
}

#[test]
fn repos_list() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "repos_list");

    let json = env.run_json(&["repos", "list", "--format", "json"]);
    assert!(
        json["items"].is_array(),
        "expected items array in: {json}",
    );
    assert!(
        json["items"].as_array().unwrap().len() > 0,
        "expected at least one repo",
    );
    assert!(
        json["total"].is_number(),
        "expected total in: {json}",
    );
}

#[test]
fn bugs_list_default() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "bugs_list_default");

    let json = env.run_json(&["bugs", "list", REPO, "--format", "json"]);
    assert!(
        json["items"].is_array(),
        "expected items array in: {json}",
    );
    assert!(json["total"].is_number(), "expected total in: {json}");
    assert!(json["page"].is_number(), "expected page in: {json}");
    assert!(
        json["total_pages"].is_number(),
        "expected total_pages in: {json}",
    );
}

#[test]
fn bugs_list_all_statuses() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "bugs_list_all_statuses");

    for status in &["pending", "resolved", "dismissed"] {
        let json = env.run_json(&["bugs", "list", REPO, "--status", status, "--format", "json"]);
        assert!(
            json["items"].is_array(),
            "expected items array for status={status}: {json}",
        );
    }
}

#[test]
fn bugs_list_pagination() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "bugs_list_pagination");

    let json = env.run_json(&[
        "bugs", "list", REPO, "--limit", "1", "--page", "1", "--format", "json",
    ]);
    let items = json["items"].as_array().unwrap();
    assert!(
        items.len() <= 1,
        "expected at most 1 item with --limit 1, got {}",
        items.len(),
    );
    assert_eq!(json["page"], 1, "expected page=1");
}

#[test]
fn bugs_list_vulns() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "bugs_list_vulns");

    let json = env.run_json(&["bugs", "list", REPO, "--vulns", "--format", "json"]);
    assert!(
        json["items"].is_array(),
        "expected items array for --vulns: {json}",
    );
}

#[test]
fn bugs_show() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "bugs_show");

    // Find a bug ID from any status
    let mut bug_id: Option<String> = None;
    for status in &["pending", "resolved", "dismissed"] {
        let json = env.run_json(&[
            "bugs", "list", REPO, "--status", status, "--limit", "1", "--format", "json",
        ]);
        if let Some(id) = json["items"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|b| b["id"].as_str())
        {
            bug_id = Some(id.to_string());
            break;
        }
    }

    let Some(bug_id) = bug_id else {
        eprintln!("no bugs found in {REPO} — skipping bugs show");
        return;
    };

    let out = env.run(&["bugs", "show", &bug_id]);
    assert!(
        out.success,
        "bugs show {bug_id} failed:\nstdout: {}\nstderr: {}",
        out.stdout,
        out.stderr,
    );
    assert!(
        out.stdout.contains(&bug_id),
        "expected bug ID {bug_id} in output: {}",
        out.stdout,
    );
}

#[test]
fn scans_list() {
    let key = require_api_key!();
    let env = Env::authenticated(&key, "scans_list");

    let json = env.run_json(&["scans", "list", REPO, "--format", "json"]);
    assert!(
        json["items"].is_array(),
        "expected items array in: {json}",
    );
    assert!(json["total"].is_number(), "expected total in: {json}");
}

#[test]
fn commands_fail_without_auth() {
    let env = Env::new("no_auth");

    let out = env.run(&["repos", "list", "--format", "json"]);
    assert!(!out.success, "repos list should fail without auth");

    let out = env.run(&["bugs", "list", REPO, "--format", "json"]);
    assert!(!out.success, "bugs list should fail without auth");

    let out = env.run(&["scans", "list", REPO, "--format", "json"]);
    assert!(!out.success, "scans list should fail without auth");
}
