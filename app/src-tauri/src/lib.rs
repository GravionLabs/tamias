//! Desktop app of tamias: a thin Tauri shell over `tamias-core`. The frontend (`app/src`) calls
//! the commands registered in [`run`] through its `Ipc` bridge (`app/src/app/core/ipc.ts`).

pub mod today;

use tamias_core::report::Report;

/// Today's report of every provider, the document of `tamias today --json` (schema version 1).
///
/// Runs off the main thread: reading the store may wait for the collector's lock.
#[tauri::command]
async fn today() -> Result<Report, String> {
    tauri::async_runtime::spawn_blocking(today::current)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Starts the app.
///
/// # Panics
///
/// When the window cannot be created.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![today])
        .run(tauri::generate_context!())
        .expect("error while running the tamias app");
}
