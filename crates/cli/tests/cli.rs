use assert_cmd::Command;
use predicates::prelude::*;

fn tamias() -> Command {
    Command::cargo_bin("tamias").expect("binary tamias is built")
}

#[test]
fn version_flag_prints_the_workspace_version() {
    tamias()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("tamias {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn short_version_flag_matches_long_one() {
    tamias()
        .arg("-V")
        .assert()
        .success()
        .stdout(format!("tamias {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_prints_usage_and_fails() {
    tamias()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage: tamias"));
}

#[test]
fn unknown_argument_is_rejected() {
    tamias()
        .arg("--no-such-flag")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "unexpected argument '--no-such-flag'",
        ));
}
