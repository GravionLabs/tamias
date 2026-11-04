//! Desktop app of tamias: a thin Tauri shell over `tamias-core`. The frontend (`app/src`) calls
//! the commands registered in [`run`] through its `Ipc` bridge (`app/src/app/core/ipc.ts`) and
//! reloads on the [`watch::EVENT`] of the live refresh (`app/src/app/core/store-events.ts`).

pub mod today;
pub mod watch;

use std::sync::{Mutex, PoisonError};

use tamias_core::paths;
use tamias_core::report::Report;
use tauri::{Emitter, Manager, RunEvent};

/// Error of a command as the frontend receives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandError {
    /// What went wrong, for the error state.
    pub message: String,
    /// The store was locked longer than its busy timeout: not an error state, the next
    /// `store-changed` event retries.
    pub busy: bool,
}

impl From<today::Error> for CommandError {
    fn from(error: today::Error) -> Self {
        Self {
            message: error.to_string(),
            busy: error.is_busy(),
        }
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

/// The live refresh, owned by the app and stopped on exit.
struct LiveRefresh(Mutex<Option<watch::Watch>>);

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

/// Starts the app.
///
/// # Panics
///
/// When the window cannot be created.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            start_live_refresh(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![today])
        .build(tauri::generate_context!())
        .expect("error while running the tamias app")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                stop_live_refresh(app);
            }
        });
}
