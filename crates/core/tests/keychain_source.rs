//! Keychain source with a fake `security` script (Unix only: the source shells out to a binary).
//!
//! One test function on purpose: writing and executing scripts from parallel threads can hit
//! `ETXTBSY` on Linux.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tamias_core::credentials::{CredentialSource, KeychainSource};
use tamias_core::provider::claude::Error;

const TOKEN: &str = "sk-ant-oat01-KEYCHAIN-TEST-TOKEN";
const SERVICE: &str = "Claude Code-credentials";

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn source(program: &Path, timeout: Duration) -> KeychainSource {
    KeychainSource::with_program(program, SERVICE, timeout)
}

#[test]
fn keychain_source_runs_security_with_timeout_and_typed_errors() {
    let dir = tempfile::tempdir().unwrap();
    let ok = script(
        dir.path(),
        "security-ok",
        &format!(
            r#"[ "$1" = find-generic-password ] && [ "$2" = -s ] && [ "$3" = "{SERVICE}" ] && [ "$4" = -w ] || exit 2
printf '%s\n' '{{"claudeAiOauth":{{"accessToken":"{TOKEN}","expiresAt":1762084800000}}}}'"#
        ),
    );
    let missing = script(dir.path(), "security-missing", "exit 44");
    let hang = script(dir.path(), "security-hang", "exec sleep 10");
    let api_key = script(
        dir.path(),
        "security-api-key",
        r#"printf '%s' '{"primaryApiKey":"x"}'"#,
    );

    // happy path: arguments as Claude Code's item needs them, trailing newline trimmed
    let secret = source(&ok, Duration::from_secs(10)).claude_token().unwrap();
    assert_eq!(
        secret.expires_at(),
        "2025-11-02T12:00:00Z".parse::<jiff::Timestamp>().unwrap()
    );

    // missing item: security exits 44
    let err = source(&missing, Duration::from_secs(10))
        .claude_token()
        .unwrap_err();
    assert!(
        matches!(err, Error::NoCredentials(ref m) if m.contains("exit 44") && m.contains(SERVICE)),
        "{err:?}"
    );

    // no OAuth entry
    let err = source(&api_key, Duration::from_secs(10))
        .claude_token()
        .unwrap_err();
    assert!(matches!(err, Error::NoCredentials(_)), "{err:?}");

    // hanging subprocess is killed at the timeout
    let started = Instant::now();
    let err = source(&hang, Duration::from_millis(300))
        .claude_token()
        .unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5));

    // program not runnable
    let err = source(&dir.path().join("does-not-exist"), Duration::from_secs(1))
        .claude_token()
        .unwrap_err();
    assert!(matches!(err, Error::NoCredentials(_)), "{err:?}");

    // the debug form of a source shows no secret, only program and service
    let shown = format!("{:?}", source(&ok, Duration::from_secs(1)));
    assert!(!shown.contains(TOKEN));
}
