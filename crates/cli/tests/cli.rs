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

/// The `Exit codes:` lines of `--help` as `(code, description)`.
fn help_exit_codes(command: &str) -> Vec<(String, String)> {
    let out = tamias()
        .args([command, "--help"])
        .output()
        .expect("help runs");
    let text = String::from_utf8(out.stdout).expect("utf-8 help");
    text.lines()
        .skip_while(|l| *l != "Exit codes:")
        .skip(1)
        .map(|l| {
            let (code, rest) = l.trim().split_once("  ").expect("code and description");
            (code.to_string(), rest.trim().to_string())
        })
        .collect()
}

#[test]
fn help_exit_codes_match_the_consumers_doc_table() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/consumers.md");
    let doc = std::fs::read_to_string(path).expect("docs/consumers.md is readable");
    for command in ["collect", "today"] {
        let codes = help_exit_codes(command);
        assert_eq!(codes.len(), 4, "{command} lists four exit codes");
        for (code, description) in codes {
            let row = format!("| {code} | {description} |");
            assert!(
                doc.contains(&row),
                "docs/consumers.md lacks `{row}` ({command})"
            );
        }
    }
}

#[test]
fn help_about_does_not_announce_copilot_as_available() {
    tamias()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude usage limits"))
        .stdout(predicate::str::contains("Copilot").not());
}
