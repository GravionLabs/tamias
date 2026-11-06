//! The launchd agent against a fake `launchctl` and a temp `LaunchAgents` directory.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tempfile::TempDir;

use super::{LABEL, LaunchAgent, plist};
use crate::service::{Output, Runner, Scheduler};

const EXPECTED_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>com.gravionlabs.tamias.collect</string>
	<key>ProgramArguments</key>
	<array>
		<string>/Applications/tamias/bin/tamias</string>
		<string>collect</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>StartInterval</key>
	<integer>300</integer>
	<key>StandardOutPath</key>
	<string>/dev/null</string>
	<key>StandardErrorPath</key>
	<string>/dev/null</string>
</dict>
</plist>
"#;

/// Fake launchd of uid 501: `bootstrap` registers the program of the plist it reads (and fails
/// when the label is already registered, like launchd), `bootout` removes it.
#[derive(Debug, Default)]
struct FakeLaunchd {
    /// label -> program of the registered agent.
    registered: RefCell<BTreeMap<String, String>>,
    calls: RefCell<Vec<String>>,
    /// `launchctl` subcommand that fails.
    fail: RefCell<Option<&'static str>>,
}

fn failed(stderr: &str) -> Output {
    Output {
        success: false,
        status: "exit status: 5".to_owned(),
        stdout: String::new(),
        stderr: stderr.to_owned(),
    }
}

fn ok(stdout: &str) -> Output {
    Output {
        success: true,
        status: "exit status: 0".to_owned(),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

impl Runner for Rc<FakeLaunchd> {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        self.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        match (program, args) {
            ("id", ["-u"]) => Ok(ok("501\n")),
            ("launchctl", [sub, ..]) if *self.fail.borrow() == Some(*sub) => {
                Ok(failed("Bootstrap failed: 5: Input/output error"))
            }
            ("launchctl", ["print", target]) => {
                let label = target.strip_prefix("gui/501/").expect("gui/501 domain");
                Ok(match self.registered.borrow().get(label) {
                    Some(program) => ok(&format!("{target} = {{\n\tprogram = {program}\n}}\n")),
                    None => failed("Could not find service"),
                })
            }
            ("launchctl", ["bootout", target]) => {
                let label = target.strip_prefix("gui/501/").expect("gui/501 domain");
                Ok(match self.registered.borrow_mut().remove(label) {
                    Some(_) => ok(""),
                    None => failed("Boot-out failed: 3: No such process"),
                })
            }
            ("launchctl", ["bootstrap", "gui/501", path]) => {
                let text = fs::read_to_string(path)?;
                let program = super::program_of_plist(&text).expect("complete plist");
                let mut registered = self.registered.borrow_mut();
                if registered.contains_key(LABEL) {
                    return Ok(failed("Bootstrap failed: 5: Input/output error"));
                }
                registered.insert(LABEL.to_owned(), program);
                Ok(ok(""))
            }
            _ => Err(io::Error::other(format!(
                "unexpected command {program} {args:?}"
            ))),
        }
    }
}

struct Fixture {
    home: TempDir,
    launchd: Rc<FakeLaunchd>,
    agent: LaunchAgent,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let launchd = Rc::new(FakeLaunchd::default());
        let agent = LaunchAgent::new(
            super::agents_dir(home.path()),
            Box::new(Rc::clone(&launchd)),
        );
        Self {
            home,
            launchd,
            agent,
        }
    }

    fn agents_dir(&self) -> PathBuf {
        super::agents_dir(self.home.path())
    }

    /// Names of all files in `LaunchAgents`.
    fn files(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.agents_dir()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn registered(&self) -> BTreeMap<String, String> {
        self.launchd.registered.borrow().clone()
    }

    fn launchctl_calls(&self) -> Vec<String> {
        self.launchd
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("launchctl ") && !c.starts_with("launchctl print"))
            .cloned()
            .collect()
    }
}

fn plist_name() -> String {
    format!("{LABEL}.plist")
}

#[test]
fn plist_runs_collect_every_300_seconds_at_load_without_keepalive() {
    assert_eq!(plist("/Applications/tamias/bin/tamias"), EXPECTED_PLIST);
}

#[test]
fn plist_escapes_the_exe_path() {
    let text = plist("/Users/a&b/<x>/tamias");
    assert!(text.contains("<string>/Users/a&amp;b/&lt;x&gt;/tamias</string>"));
    assert_eq!(
        super::program_of_plist(&text).as_deref(),
        Some("/Users/a&b/<x>/tamias")
    );
}

#[test]
fn install_writes_the_plist_and_bootstraps_it() {
    let f = Fixture::new();
    let exe = Path::new("/opt/tamias/tamias");
    f.agent.install(exe).unwrap();

    assert_eq!(f.files(), vec![plist_name()]);
    assert_eq!(
        fs::read_to_string(f.agent.plist_path()).unwrap(),
        plist("/opt/tamias/tamias")
    );
    assert_eq!(
        f.launchctl_calls(),
        vec![format!(
            "launchctl bootstrap gui/501 {}",
            f.agent.plist_path().display()
        )]
    );
    let state = f.agent.status().unwrap();
    assert!(state.installed && state.registered);
    assert_eq!(state.exe, Some(PathBuf::from("/opt/tamias/tamias")));
    assert_eq!(state.definition, Some(f.agent.plist_path()));
}

#[test]
fn install_writes_the_plist_with_mode_644_even_over_a_stale_temp_file() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    fs::create_dir_all(f.agents_dir()).unwrap();
    let stale = f.agents_dir().join(format!(".{LABEL}.plist.tmp"));
    fs::write(&stale, "stale").unwrap();
    fs::set_permissions(&stale, fs::Permissions::from_mode(0o600)).unwrap();

    f.agent.install(Path::new("/opt/tamias/tamias")).unwrap();

    let mode = fs::metadata(f.agent.plist_path())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o644);
}

#[test]
fn install_is_idempotent_and_the_last_exe_wins() {
    let f = Fixture::new();
    for exe in [
        "/old/tamias",
        "/new/tamias",
        "/new/tamias",
        "/current/tamias",
    ] {
        f.agent.install(Path::new(exe)).unwrap();
        assert_eq!(f.files(), vec![plist_name()], "after installing {exe}");
        assert_eq!(
            f.registered(),
            BTreeMap::from([(LABEL.to_owned(), exe.to_owned())]),
            "after installing {exe}"
        );
    }
    let calls = f.launchctl_calls();
    assert_eq!(calls.iter().filter(|c| c.contains("bootstrap")).count(), 4);
    assert_eq!(calls.iter().filter(|c| c.contains("bootout")).count(), 3);
    assert_eq!(
        f.agent.status().unwrap().exe,
        Some(PathBuf::from("/current/tamias"))
    );
}

#[test]
fn install_replaces_a_plist_left_without_registration() {
    let f = Fixture::new();
    fs::create_dir_all(f.agents_dir()).unwrap();
    fs::write(f.agent.plist_path(), "stale").unwrap();
    f.agent.install(Path::new("/x/tamias")).unwrap();
    assert_eq!(
        fs::read_to_string(f.agent.plist_path()).unwrap(),
        plist("/x/tamias")
    );
    assert_eq!(f.registered().len(), 1);
}

#[test]
fn failing_bootout_leaves_the_existing_plist_untouched_and_no_temp_file() {
    let f = Fixture::new();
    f.agent.install(Path::new("/old/tamias")).unwrap();
    *f.launchd.fail.borrow_mut() = Some("bootout");

    let err = f.agent.install(Path::new("/new/tamias")).unwrap_err();
    assert!(err.to_string().contains("launchctl bootout"), "{err}");
    assert_eq!(f.files(), vec![plist_name()]);
    assert_eq!(
        fs::read_to_string(f.agent.plist_path()).unwrap(),
        plist("/old/tamias")
    );
}

#[test]
fn failing_bootstrap_leaves_no_plist_behind() {
    let f = Fixture::new();
    *f.launchd.fail.borrow_mut() = Some("bootstrap");

    let err = f.agent.install(Path::new("/x/tamias")).unwrap_err();
    assert!(err.to_string().contains("Input/output error"), "{err}");
    assert!(f.files().is_empty(), "{:?}", f.files());
    assert!(f.registered().is_empty());
    assert!(!f.agent.status().unwrap().installed);
}

#[test]
fn uninstall_removes_the_registration_and_the_plist_but_keeps_the_data() {
    let f = Fixture::new();
    let data = f.home.path().join("Library/Application Support/tamias");
    fs::create_dir_all(data.join("logs")).unwrap();
    fs::write(data.join("logs/collect.log"), "line\n").unwrap();
    fs::write(data.join("tamias.db"), "db").unwrap();

    f.agent.install(Path::new("/x/tamias")).unwrap();
    f.agent.uninstall().unwrap();

    assert_eq!(f.files(), [] as [std::string::String; 0]);
    assert!(f.registered().is_empty());
    assert_eq!(
        fs::read_to_string(data.join("logs/collect.log")).unwrap(),
        "line\n"
    );
    assert!(data.join("tamias.db").exists());
    let state = f.agent.status().unwrap();
    assert!(!state.installed && !state.registered);
    assert_eq!(state.exe, None);
}

#[test]
fn uninstall_when_not_installed_is_ok_and_calls_no_bootout() {
    let f = Fixture::new();
    f.agent.uninstall().unwrap();
    assert_eq!(f.launchctl_calls(), [] as [std::string::String; 0]);
}

#[test]
fn uninstall_removes_a_registration_without_plist() {
    let f = Fixture::new();
    f.launchd
        .registered
        .borrow_mut()
        .insert(LABEL.to_owned(), "/gone/tamias".to_owned());
    let state = f.agent.status().unwrap();
    assert!(state.installed && state.registered);
    assert_eq!(state.exe, Some(PathBuf::from("/gone/tamias")));
    assert_eq!(state.definition, None);

    f.agent.uninstall().unwrap();
    assert!(f.registered().is_empty());
}

#[test]
fn failing_bootout_on_uninstall_keeps_the_plist() {
    let f = Fixture::new();
    f.agent.install(Path::new("/x/tamias")).unwrap();
    *f.launchd.fail.borrow_mut() = Some("bootout");
    assert!(f.agent.uninstall().is_err());
    assert_eq!(f.files(), vec![plist_name()]);
}

#[test]
fn status_of_a_plist_that_is_not_loaded() {
    let f = Fixture::new();
    fs::create_dir_all(f.agents_dir()).unwrap();
    fs::write(f.agent.plist_path(), plist("/y/tamias")).unwrap();
    let state = f.agent.status().unwrap();
    assert!(state.installed);
    assert!(!state.registered);
    assert_eq!(state.exe, Some(PathBuf::from("/y/tamias")));
}
