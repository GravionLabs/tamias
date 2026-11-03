//! Windows: a Task Scheduler task in the user's context (no admin, no stored password).
//!
//! The task `\GravionLabs\tamias-collect` is registered with `schtasks /Create /XML` and runs
//! `tamiasw.exe` (the windowless `collect`, next to `tamias.exe`) every 5 minutes, also as soon as
//! possible after a missed run (sleep). `/F` replaces an existing task, so install is idempotent.

use std::fs;
use std::path::{Path, PathBuf};

use super::{Error, Result, Runner, Scheduler, SchedulerState, run_checked};

/// Full name of the task, including its folder.
pub const TASK_NAME: &str = "\\GravionLabs\\tamias-collect";
/// The windowless collector that the task runs.
pub const COLLECTOR_EXE: &str = "tamiasw.exe";

const SCHTASKS: &str = "schtasks";
const XML_FILE: &str = "tamias-collect-task.xml";

/// The `tamiasw.exe` that belongs to `tamias_exe` (same directory).
#[must_use]
pub fn collector_of(tamias_exe: &Path) -> PathBuf {
    tamias_exe.with_file_name(COLLECTOR_EXE)
}

/// The Task Scheduler task of the collector.
pub struct TaskScheduler {
    xml_dir: PathBuf,
    runner: Box<dyn Runner>,
}

impl std::fmt::Debug for TaskScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskScheduler")
            .field("xml_dir", &self.xml_dir)
            .finish_non_exhaustive()
    }
}

impl TaskScheduler {
    /// Task whose definition file is written to `xml_dir` for the time of the registration.
    #[must_use]
    pub fn new(xml_dir: PathBuf, runner: Box<dyn Runner>) -> Self {
        Self { xml_dir, runner }
    }

    /// `schtasks /Query /XML` of the task: its XML, or `None` when there is no such task.
    fn query(&self) -> Result<Option<String>> {
        let args = ["/Query", "/TN", TASK_NAME, "/XML"];
        let output = self
            .runner
            .run(SCHTASKS, &args)
            .map_err(|e| Error::io("cannot run `schtasks /Query`", e))?;
        // schtasks has no distinct exit code (and localised messages) for a missing task.
        Ok(output.success.then_some(output.stdout))
    }
}

impl Scheduler for TaskScheduler {
    fn install(&self, exe: &Path) -> Result<()> {
        let collector = collector_of(exe);
        if !collector.is_file() {
            return Err(Error::Other(format!(
                "{} not found next to {}; install both executables in the same directory",
                COLLECTOR_EXE,
                exe.display()
            )));
        }
        let collector = collector
            .to_str()
            .ok_or_else(|| Error::Other(format!("{} is not valid UTF-8", collector.display())))?;
        let xml_path = self.xml_dir.join(XML_FILE);
        let xml_arg = xml_path
            .to_str()
            .ok_or_else(|| Error::Other(format!("{} is not valid UTF-8", xml_path.display())))?;
        fs::write(&xml_path, encode_utf16(&task_xml(collector)))
            .map_err(|e| Error::io(format!("cannot write {}", xml_path.display()), e))?;
        let created = run_checked(
            self.runner.as_ref(),
            SCHTASKS,
            &["/Create", "/XML", xml_arg, "/TN", TASK_NAME, "/F"],
        );
        let _ = fs::remove_file(&xml_path);
        created.map(drop)
    }

    fn uninstall(&self) -> Result<()> {
        if self.query()?.is_some() {
            run_checked(
                self.runner.as_ref(),
                SCHTASKS,
                &["/Delete", "/TN", TASK_NAME, "/F"],
            )?;
        }
        Ok(())
    }

    fn status(&self) -> Result<SchedulerState> {
        let xml = self.query()?;
        Ok(SchedulerState {
            installed: xml.is_some(),
            registered: xml
                .as_deref()
                .is_some_and(|x| !x.contains("<Enabled>false</Enabled>")),
            exe: xml.as_deref().and_then(command_of).map(PathBuf::from),
            definition: None,
        })
    }
}

/// The task definition: every 5 minutes without end, catch up after a missed run, also on battery.
#[must_use]
pub fn task_xml(collector: &str) -> String {
    let command = escape(collector);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
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
      <Command>{command}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// UTF-16 little endian with a byte order mark, which `schtasks /XML` expects for the declared
/// `encoding="UTF-16"`.
#[must_use]
pub fn encode_utf16(text: &str) -> Vec<u8> {
    std::iter::once(0xFEFF_u16)
        .chain(text.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The `<Command>` of the (queried) task XML.
fn command_of(xml: &str) -> Option<String> {
    let rest = xml.split_once("<Command>")?.1;
    let command = rest.split_once("</Command>")?.0.trim();
    (!command.is_empty()).then(|| unescape(command))
}

#[cfg(test)]
mod tests;
