//! Tests de bout en bout du binaire `redm-linux`.

use std::io::Write;
use std::process::{Command, Stdio};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_redm-linux"))
}

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/synthetic-wine.log"
);

#[test]
fn analyze_log_json_is_an_evidence_record() {
    let out = bin()
        .args(["--json", "analyze-log", FIXTURE])
        .output()
        .unwrap();
    // Le journal contient des constats `error` → code 2.
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["kind"], "log-analysis");
    assert_eq!(v["schema_version"], 1);
    assert!(["native-linux", "wsl2", "wsl1", "unknown"].contains(&v["host"].as_str().unwrap()));
    assert_eq!(v["payload"]["lines_scanned"], 5);
    let ids: Vec<&str> = v["payload"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["rule_id"].as_str().unwrap())
        .collect();
    for expected in [
        "wine.missing-dll",
        "wine.dll-not-found-status",
        "wine.ntlm-auth-missing",
        "wine.stub",
    ] {
        assert!(ids.contains(&expected), "{expected} absent de {ids:?}");
    }
}

#[test]
fn analyze_log_from_stdin_blocker_exit_code() {
    let mut child = bin()
        .args(["analyze-log", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Adhesive: initialization failed\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stdout).contains("anticheat.adhesive"));
}

#[test]
fn missing_file_is_a_tool_failure() {
    let out = bin()
        .args(["analyze-log", "/nonexistent/redm.log"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn env_json_reports_host() {
    let out = bin().args(["--json", "env"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["kind"], "environment");
    assert_eq!(v["host"], v["payload"]["host"]);
}

/// Chaque sous-commande doit produire un JSON valide (régression : `runtimes`
/// et `scan` échouaient sur `#[serde(flatten)]` d'une liste).
#[test]
fn every_command_emits_valid_json() {
    let tmp = std::env::temp_dir();
    let tmp = tmp.to_str().unwrap();
    for args in [
        vec!["env"],
        vec!["runtimes"],
        vec!["prefix", tmp],
        vec!["scan"],
        vec!["doctor"],
        vec!["analyze-log", FIXTURE],
    ] {
        let out = bin().arg("--json").args(&args).output().unwrap();
        assert_ne!(
            out.status.code(),
            Some(1),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: JSON invalide : {e}"));
        assert!(v["payload"]["findings"].is_array(), "{args:?}");
    }
}
