//! NetTune backend: Tauri command layer. All work runs on blocking worker threads
//! so the UI stays responsive; a single busy flag prevents overlapping runs/changes.

pub mod cmd;
pub mod compare;
pub mod detect;
pub mod diagnose;
pub mod models;
pub mod powersave;
pub mod probe;
pub mod rules;

use cmd::SystemRunner;
use compare::Comparison;
use models::*;
use powersave::BackupStore;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

struct AppState {
    busy: AtomicBool,
    store: BackupStore,
}

/// Holds the busy flag for the lifetime of one operation.
struct BusyGuard(Arc<AppState>);

impl BusyGuard {
    fn acquire(state: &Arc<AppState>, what: &str) -> Result<Self, String> {
        state
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| BusyGuard(state.clone()))
            .map_err(|_| format!("Another operation is in progress; {what} was not started."))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("internal error: {e}"))?
}

fn find_interface(name: &str) -> Result<NetInterface, String> {
    detect::list_interfaces()?
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("Interface {name} is no longer present."))
}

#[tauri::command]
async fn list_interfaces() -> Result<Vec<NetInterface>, String> {
    blocking(detect::list_interfaces).await
}

#[tauri::command]
fn estimate_duration(config: DiagnosticConfig) -> u64 {
    diagnose::estimated_seconds(config)
}

#[tauri::command]
async fn run_diagnostics(
    state: State<'_, Arc<AppState>>,
    interface: String,
    config: DiagnosticConfig,
) -> Result<DiagnosticReport, String> {
    let st = state.inner().clone();
    let guard = BusyGuard::acquire(&st, "diagnostics")?;
    blocking(move || {
        let _g = guard;
        diagnose::run(&interface, config, &st.store)
    })
    .await
}

#[tauri::command]
async fn power_save_state(
    state: State<'_, Arc<AppState>>,
    interface: String,
) -> Result<PowerSaveState, String> {
    let st = state.inner().clone();
    blocking(move || {
        Ok(powersave::state(
            &find_interface(&interface)?,
            &SystemRunner,
            &st.store,
        ))
    })
    .await
}

#[tauri::command]
async fn apply_power_save(
    state: State<'_, Arc<AppState>>,
    interface: String,
) -> Result<ChangeResult, String> {
    let st = state.inner().clone();
    let guard = BusyGuard::acquire(&st, "the change")?;
    blocking(move || {
        let _g = guard;
        let iface = find_interface(&interface)?;
        let outcome =
            powersave::apply(&iface, &SystemRunner, &st.store, Duration::from_millis(500))?;
        // Re-detect: the connection was re-activated.
        let state = find_interface(&interface)
            .map(|i| powersave::state(&i, &SystemRunner, &st.store))
            .unwrap_or_else(|_| powersave::state(&iface, &SystemRunner, &st.store));
        Ok(ChangeResult {
            ok: outcome.ok,
            message: outcome.message,
            state,
        })
    })
    .await
}

#[tauri::command]
async fn revert_power_save(
    state: State<'_, Arc<AppState>>,
    interface: String,
) -> Result<ChangeResult, String> {
    let st = state.inner().clone();
    let guard = BusyGuard::acquire(&st, "the revert")?;
    blocking(move || {
        let _g = guard;
        let outcome = powersave::revert(&SystemRunner, &st.store, Duration::from_millis(500))?;
        let state = powersave::state(&find_interface(&interface)?, &SystemRunner, &st.store);
        Ok(ChangeResult {
            ok: outcome.ok,
            message: outcome.message,
            state,
        })
    })
    .await
}

#[tauri::command]
fn compare_reports(before: DiagnosticReport, after: DiagnosticReport) -> Comparison {
    compare::compare(&before, &after)
}

pub fn run() {
    let state = Arc::new(AppState {
        busy: AtomicBool::new(false),
        store: BackupStore::default_location(),
    });
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            list_interfaces,
            estimate_duration,
            run_diagnostics,
            power_save_state,
            apply_power_save,
            revert_power_save,
            compare_reports
        ])
        .run(tauri::generate_context!())
        .expect("error while running NetTune");
}
