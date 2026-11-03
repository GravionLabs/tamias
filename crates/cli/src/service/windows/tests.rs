//! The Task Scheduler task against a fake `schtasks` and a temp directory.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tempfile::TempDir;

use super::{TASK_NAME, TaskScheduler, collector_of, encode_utf16, task_xml};
use crate::service::{Output, Runner, Scheduler};

const EXPECTED_XML: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>tamias collector: polls Claude and Copilot usage every 5 minutes</Description>
  </RegistrationInfo>
  <Triggers>
    <TimeTrigger>
      <Repetition>
        <Interval>PT5M</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>2026-01-01T00:00:00</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <ExecutionTimeLimit>PT2M</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>C:\Tools\tamias\tamiasw.exe</Command>
    </Exec>
  </Actions>
</Task>
"#;

fn result(success: bool, stdout: &str, stderr: &str) -> Output {
    Output {
        success,
        status: if success {
            "exit status: 0"
        } else {
            "exit status: 1"
        }
        .to_owned(),
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
    }
}

/// Fake `schtasks`: `/Create` stores the XML file it is given (decoded), `/Delete` clears it.
#[derive(Debug, Default)]
struct FakeSchtasks {
    task: RefCell<Option<String>>,
    calls: RefCell<Vec<String>>,
    /// The XML file bytes seen by `/Create`.
    created_bytes: RefCell<Vec<Vec<u8>>>,
    fail_create: RefCell<bool>,
    missing: RefCell<bool>,
}

#[derive(Debug, Clone, Default)]
struct Shared(Rc<FakeSchtasks>);

impl Runner for Shared {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Output> {
        let fake = &self.0;
        if *fake.missing.borrow() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        fake.calls
            .borrow_mut()
            .push(format!("{program} {}", args.join(" ")));
        match args {
            ["/Create", "/XML", file, "/TN", name, "/F"] => {
                assert_eq!(*name, TASK_NAME);
                if *fake.fail_create.borrow() {
                    return Ok(result(false, "", "ERROR: Access is denied."));
                }
                let bytes = fs::read(file).expect("xml file exists during /Create");
                let units: Vec<u16> = bytes
                    .chunks(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                *fake.task.borrow_mut() = Some(String::from_utf16(&units[1..]).unwrap());
                fake.created_bytes.borrow_mut().push(bytes);
                Ok(result(true, "SUCCESS", ""))
            }
            ["/Query", "/TN", _, "/XML"] => Ok(match fake.task.borrow().as_ref() {
                Some(xml) => result(true, xml, ""),
                None => result(
                    false,
                    "",
                    "ERROR: The system cannot find the file specified.",
                ),
            }),
            ["/Delete", "/TN", _, "/F"] => {
                *fake.task.borrow_mut() = None;
                Ok(result(true, "SUCCESS", ""))
            }
            other => panic!("unexpected schtasks call {other:?}"),
        }
    }
}

struct Setup {
    dir: TempDir,
    fake: Shared,
    scheduler: TaskScheduler,
}

impl Setup {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("xml")).unwrap();
        let fake = Shared::default();
        let scheduler = TaskScheduler::new(dir.path().join("xml"), Box::new(fake.clone()));
        Self {
            dir,
            fake,
            scheduler,
        }
    }

    /// A `tamias.exe` with (or without) its `tamiasw.exe` neighbour.
    fn exe(&self, with_collector: bool) -> PathBuf {
        let bin = self.dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("tamias.exe"), "").unwrap();
        if with_collector {
            fs::write(bin.join("tamiasw.exe"), "").unwrap();
        }
        bin.join("tamias.exe")
    }

    fn xml_files(&self) -> usize {
        fs::read_dir(self.dir.path().join("xml")).unwrap().count()
    }
}

#[test]
fn task_xml_is_exactly_the_documented_definition() {
    assert_eq!(task_xml(r"C:\Tools\tamias\tamiasw.exe"), EXPECTED_XML);
}

#[test]
fn task_xml_escapes_the_command() {
    let xml = task_xml(r"C:\R&D <x>\tamiasw.exe");
    assert!(
        xml.contains(r"<Command>C:\R&amp;D &lt;x&gt;\tamiasw.exe</Command>"),
        "{xml}"
    );
}

#[test]
fn utf16_is_little_endian_with_a_byte_order_mark() {
    assert_eq!(
        encode_utf16("a\u{e9}\u{1f43f}"),
        [0xFF, 0xFE, 0x61, 0x00, 0xE9, 0x00, 0x3D, 0xD8, 0x3F, 0xDC]
    );
}

#[test]
fn collector_is_the_neighbour_of_tamias() {
    let dir = Path::new("tools");
    assert_eq!(
        collector_of(&dir.join("tamias.exe")),
        dir.join("tamiasw.exe")
    );
}

#[test]
fn install_registers_the_task_with_a_utf16_file_and_removes_the_file() {
    let s = Setup::new();
    let exe = s.exe(true);
    s.scheduler.install(&exe).unwrap();

    let calls = s.fake.0.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("schtasks /Create /XML "), "{calls:?}");
    assert!(
        calls[0].ends_with(r" /TN \GravionLabs\tamias-collect /F"),
        "{calls:?}"
    );
    let bytes = s.fake.0.created_bytes.borrow()[0].clone();
    assert_eq!(&bytes[..2], [0xFF, 0xFE]);
    let collector = collector_of(&exe);
    let xml = s.fake.0.task.borrow().clone().unwrap();
    assert_eq!(xml, task_xml(collector.to_str().unwrap()));
    assert_eq!(s.xml_files(), 0, "the temp XML is removed");
}

#[test]
fn install_twice_replaces_the_task() {
    let s = Setup::new();
    let exe = s.exe(true);
    s.scheduler.install(&exe).unwrap();
    s.scheduler.install(&exe).unwrap();
    let calls = s.fake.0.calls.borrow();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().all(|c| c.ends_with(" /F")));
    assert!(s.fake.0.task.borrow().is_some());
}

#[test]
fn install_fails_clearly_without_tamiasw_and_runs_nothing() {
    let s = Setup::new();
    let exe = s.exe(false);
    let err = s.scheduler.install(&exe).unwrap_err().to_string();
    assert!(err.contains("tamiasw.exe not found next to"), "{err}");
    assert!(s.fake.0.calls.borrow().is_empty());
}

#[test]
fn install_reports_a_failing_schtasks_and_leaves_no_file() {
    let s = Setup::new();
    let exe = s.exe(true);
    *s.fake.0.fail_create.borrow_mut() = true;
    let err = s.scheduler.install(&exe).unwrap_err().to_string();
    assert!(err.contains("failed"), "{err}");
    assert!(err.contains("Access is denied"), "{err}");
    assert_eq!(s.xml_files(), 0);
}

#[test]
fn install_reports_a_missing_schtasks() {
    let s = Setup::new();
    let exe = s.exe(true);
    *s.fake.0.missing.borrow_mut() = true;
    let err = s.scheduler.install(&exe).unwrap_err().to_string();
    assert!(err.contains("cannot run"), "{err}");
}

#[test]
fn uninstall_deletes_the_task_but_not_the_data() {
    let s = Setup::new();
    let exe = s.exe(true);
    let data = s.dir.path().join("data");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("tamias.db"), "x").unwrap();
    s.scheduler.install(&exe).unwrap();
    s.scheduler.uninstall().unwrap();
    assert!(s.fake.0.task.borrow().is_none());
    assert!(
        s.fake
            .0
            .calls
            .borrow()
            .last()
            .unwrap()
            .ends_with(r"/Delete /TN \GravionLabs\tamias-collect /F")
    );
    assert!(data.join("tamias.db").exists());
}

#[test]
fn uninstall_without_a_task_is_a_no_op() {
    let s = Setup::new();
    s.scheduler.uninstall().unwrap();
    assert_eq!(s.fake.0.calls.borrow().len(), 1, "only the query ran");
}

#[test]
fn status_without_a_task_is_not_installed() {
    let s = Setup::new();
    let state = s.scheduler.status().unwrap();
    assert!(!state.installed && !state.registered);
    assert_eq!(state.exe, None);
}

#[test]
fn status_reads_the_registered_exe() {
    let s = Setup::new();
    let exe = s.exe(true);
    s.scheduler.install(&exe).unwrap();
    let state = s.scheduler.status().unwrap();
    assert!(state.installed && state.registered);
    assert_eq!(state.exe, Some(collector_of(&exe)));
}

#[test]
fn status_of_a_disabled_task_is_not_registered() {
    let s = Setup::new();
    *s.fake.0.task.borrow_mut() = Some(
        "<Task><Settings><Enabled>false</Enabled></Settings><Command>C:\\a &amp; b\\tamiasw.exe</Command></Task>"
            .to_owned(),
    );
    let state = s.scheduler.status().unwrap();
    assert!(state.installed && !state.registered);
    assert_eq!(state.exe, Some(PathBuf::from(r"C:\a & b\tamiasw.exe")));
}
