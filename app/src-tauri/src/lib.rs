//! Desktop app of tamias: a thin Tauri shell over `tamias-core`. The frontend (`app/src`) calls
//! the commands registered in [`run`] through its `Ipc` bridge (`app/src/app/core/ipc.ts`) and
//! reloads on the [`watch::EVENT`] of the live refresh (`app/src/app/core/store-events.ts`).
//! The [`tray`] icon shows the traffic light; closing the window hides it to the tray.
//! The [`service`] commands set up collection with the bundled `tamias` CLI.

pub mod autostart;
pub mod day;
pub mod launch;
pub mod month;
pub mod service;
pub mod today;
pub mod tray;
pub mod watch;

use std::sync::{Mutex, PoisonError};

use tamias_core::paths;
use tamias_core::report::Report;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

/// Error of a command as the frontend receives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandError {
    /// What went wrong, for the error state.
    pub message: String,
    /// The store was locked longer than its busy timeout: not an error state, the next
    /// `store-changed` event retries.
    pub busy: bool,
}

impl From<day::Error> for CommandError {
    fn from(error: day::Error) -> Self {
        Self {
            message: error.to_string(),
            busy: error.is_busy(),
        }
    }
}

impl From<today::Error> for CommandError {
    fn from(error: today::Error) -> Self {
        Self {
            message: error.to_string(),
            busy: error.is_busy(),
        }
    }
}

impl From<service::Error> for CommandError {
    fn from(error: service::Error) -> Self {
        Self {
            message: error.to_string(),
            busy: false,
        }
    }
}

fn join_error(error: &tauri::Error) -> CommandError {
    CommandError {
        message: error.to_string(),
        busy: false,
    }
}

/// Today's report of every provider, the document of `tamias today --json` (schema version 1).
///
/// Runs off the main thread: reading the store may wait for the collector's lock.
#[tauri::command]
async fn today() -> Result<Report, CommandError> {
    tauri::async_runtime::spawn_blocking(today::current)
        .await
        .map_err(|e| CommandError {
            message: e.to_string(),
            busy: false,
        })?
        .map_err(CommandError::from)
}

/// The month of every provider: the days with usage against the budget, and the status of each
/// provider. `month` is 1-12.
///
/// Runs off the main thread: reading the store may wait for the collector's lock.
#[tauri::command]
async fn month(year: i16, month: i8) -> Result<month::MonthReport, CommandError> {
    tauri::async_runtime::spawn_blocking(move || month::current(year, month))
        .await
        .map_err(|e| CommandError {
            message: e.to_string(),
            busy: false,
        })?
        .map_err(CommandError::from)
}

/// The intraday curve of the local day `date` (`YYYY-MM-DD`) of every provider, with the day's
/// budget and usage.
///
/// Runs off the main thread: reading the store may wait for the collector's lock.
#[tauri::command]
async fn day(date: String) -> Result<day::DayReport, CommandError> {
    tauri::async_runtime::spawn_blocking(move || day::current(&date))
        .await
        .map_err(|e| CommandError {
            message: e.to_string(),
            busy: false,
        })?
        .map_err(CommandError::from)
}

/// Whether collection is set up: `tamias service status` of the bundled CLI, mapped to
/// `installed | not_installed | error`. Never rejects: every failure, a timeout included, is the
/// `error` state with its message.
#[tauri::command]
async fn service_status() -> service::ServiceStatus {
    tauri::async_runtime::spawn_blocking(|| match service::Sidecar::bundled() {
        Ok(sidecar) => sidecar.status(),
        Err(error) => service::ServiceStatus {
            state: service::State::Error,
            message: Some(error.to_string()),
            exe: None,
        },
    })
    .await
    .unwrap_or_else(|error| service::ServiceStatus {
        state: service::State::Error,
        message: Some(error.to_string()),
        exe: None,
    })
}

/// Sets up collection: `tamias service install` of the bundled CLI (on Linux of its copy in
/// `~/.local/bin`). Rejects while another install or uninstall runs. The frontend reads the
/// status again afterwards, also after a failure.
#[tauri::command]
async fn service_install(
    lock: tauri::State<'_, service::ServiceLock>,
) -> Result<String, CommandError> {
    let guard = lock.try_lock()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        service::Sidecar::bundled()?.install()
    })
    .await
    .map_err(|e| join_error(&e))?
    .map_err(CommandError::from)
}

/// Removes collection: `tamias service uninstall` of the bundled CLI (the data and the log are
/// kept). Shares the lock of [`service_install`].
#[tauri::command]
async fn service_uninstall(
    lock: tauri::State<'_, service::ServiceLock>,
) -> Result<String, CommandError> {
    let guard = lock.try_lock()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        service::Sidecar::bundled()?.uninstall()
    })
    .await
    .map_err(|e| join_error(&e))?
    .map_err(CommandError::from)
}

/// The live refresh, owned by the app and stopped on exit.
struct LiveRefresh(Mutex<Option<watch::Watch>>);

/// Updates the tray from a blocking thread: the read waits for the store, the update for the main
/// thread, and neither may hold up the watcher thread (joined on exit) or the main thread.
fn refresh_tray(app: &tauri::AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || tray::refresh(&handle));
}

/// Starts the watch of the data directory; without one there is nothing to refresh.
fn start_live_refresh(app: &tauri::AppHandle) {
    let data_dir = match paths::data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("tamias-app: live refresh: {error}");
            return;
        }
    };
    let handle = app.clone();
    let emit = move |_| {
        refresh_tray(&handle);
        if let Err(error) = handle.emit(watch::EVENT, ()) {
            eprintln!(
                "tamias-app: live refresh: cannot emit {}: {error}",
                watch::EVENT
            );
        }
    };
    match watch::Watch::start(data_dir, watch::Options::default(), emit) {
        Ok(live) => {
            app.manage(LiveRefresh(Mutex::new(Some(live))));
        }
        Err(error) => eprintln!("tamias-app: live refresh: cannot start: {error}"),
    }
}

fn stop_live_refresh(app: &tauri::AppHandle) {
    let Some(live) = app.try_state::<LiveRefresh>() else {
        return;
    };
    let watch = live.0.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(watch) = watch {
        watch.stop();
    }
}

/// Whether autostart can be enabled: false in a dev build, whose executable cannot run on its own.
#[tauri::command]
fn autostart_available() -> bool {
    autostart::available()
}

/// Creates the autostart entry; refused in a dev build.
#[tauri::command]
fn autostart_enable(app: tauri::AppHandle) -> Result<(), CommandError> {
    autostart::check_enable(!autostart::available()).map_err(|message| CommandError {
        message: message.to_owned(),
        busy: false,
    })?;
    app.autolaunch().enable().map_err(|e| CommandError {
        message: e.to_string(),
        busy: false,
    })
}

/// Starts the app.
///
/// # Panics
///
/// When the window cannot be created.
pub fn run() {
    tauri::Builder::default()
        // The entry starts the app with `--hidden`; the OS is the source of truth for whether it
        // exists, there is no own setting.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![launch::HIDDEN]),
        ))
        .setup(|app| {
            let tray = tray::create(app.handle());
            if let Err(error) = &tray {
                eprintln!("tamias-app: tray: cannot create: {error}");
            }
            start_live_refresh(app.handle());
            refresh_tray(app.handle());
            // The window is created hidden (tauri.conf.json) and shown unless `--hidden`; without
            // a tray there is no way to open it later, so it is shown then.
            if tray.is_err() || launch::initial_visibility(std::env::args().skip(1)) {
                tray::show_window(app.handle());
            } else {
                tray::hide_dock_icon(app.handle());
            }
            Ok(())
        })
        .manage(service::ServiceLock::default())
        .on_window_event(|window, event| {
            // Closing hides to the tray, "Quit" in the tray menu exits; without a tray the
            // window closes and the app exits.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == tray::MAIN_WINDOW
                && tray::exists(window.app_handle())
            {
                api.prevent_close();
                tray::hide_window(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            today,
            month,
            day,
            service_status,
            service_install,
            service_uninstall,
            autostart_available,
            autostart_enable
        ])
        .build(tauri::generate_context!())
        .expect("error while running the tamias app")
        .run(|app, event| match event {
            // "Quit" in the tray and every other exit: the watcher thread is stopped and joined.
            RunEvent::Exit => stop_live_refresh(app),
            // The dock icon of a hidden window (should it still be shown) opens the window.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => tray::show_window(app),
            _ => {}
        });
}
