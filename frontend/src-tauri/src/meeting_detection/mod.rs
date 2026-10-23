//! Native polling owns timing; the global webview only prepares transcript state.
mod budget;
mod controls;
mod native;
mod policy;
mod postprocessing;
mod preparation;
use crate::audio::{recording_commands as recording, recording_preferences};
use policy::{Action, Observation, Policy};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_store::StoreExt;

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub teams: bool,
    pub zoom: bool,
}
#[derive(Clone, Serialize)]
pub struct Status {
    preferences: Preferences,
    supported: bool,
    accessibility: bool,
    teams: String,
    zoom: String,
    error: Option<String>,
    waiting_for_save: bool,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            preferences: Preferences::default(),
            supported: cfg!(target_os = "macos"),
            accessibility: false,
            teams: "unknown".into(),
            zoom: "unknown".into(),
            error: None,
            waiting_for_save: false,
        }
    }
}
#[derive(Default)]
pub struct Automation {
    status: Mutex<Status>,
    ready: std::sync::atomic::AtomicBool,
    relinquish: std::sync::atomic::AtomicBool,
    generation: std::sync::atomic::AtomicU64,
    processing_session: postprocessing::PostProcessing,
    preparation: Mutex<Option<(preparation::Lease, tokio::sync::oneshot::Sender<()>)>>,
}
#[tauri::command]
pub fn get_automatic_recording_status(state: tauri::State<'_, Automation>) -> Status {
    state.status.lock().unwrap().clone()
}
#[tauri::command]
pub fn set_automatic_recording_preferences<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Automation>,
    preferences: Preferences,
) -> Result<(), String> {
    let store = app
        .store("automatic_recording.json")
        .map_err(|e| e.to_string())?;
    let old = store.get("preferences");
    store.set(
        "preferences",
        serde_json::to_value(&preferences).map_err(|e| e.to_string())?,
    );
    if let Err(e) = store.save() {
        if let Some(old) = old {
            store.set("preferences", old);
        } else {
            store.delete("preferences");
        }
        return Err(e.to_string());
    }
    let mut status = state.status.lock().unwrap();
    if (status.preferences.teams && !preferences.teams)
        || (status.preferences.zoom && !preferences.zoom)
    {
        state
            .relinquish
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    if status.preferences != preferences {
        state
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    status.preferences = preferences;
    Ok(())
}
#[tauri::command]
pub fn automatic_recording_frontend_ready(state: tauri::State<'_, Automation>, ready: bool) {
    state
        .ready
        .store(ready, std::sync::atomic::Ordering::SeqCst);
    if !ready {
        state.preparation.lock().unwrap().take();
    }
}
pub(crate) fn generation_current<R: Runtime>(app: &AppHandle<R>, generation: u64) -> bool {
    app.try_state::<Automation>()
        .map(|state| {
            state.generation.load(std::sync::atomic::Ordering::SeqCst) == generation
                && state.ready.load(std::sync::atomic::Ordering::SeqCst)
                && state.processing_session.pending() == 0
        })
        .unwrap_or(false)
}
pub(crate) fn ensure_recording_start_available<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), String> {
    if let Some(state) = app.try_state::<Automation>() {
        state.processing_session.ensure_start_available()?;
    }
    Ok(())
}
pub(crate) fn recording_stopped<R: Runtime>(app: &AppHandle<R>, session: u64) {
    if let Some(state) = app.try_state::<Automation>() {
        if let Err(error) = state.processing_session.begin(session) {
            log::error!("Could not reserve recording post-processing: {error}");
        }
    }
}
#[tauri::command]
pub fn complete_recording_postprocessing(state: tauri::State<'_, Automation>, session: u64) {
    state.processing_session.complete(session);
}
#[tauri::command]
pub fn retry_recording_postprocessing<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, Automation>,
) -> Result<(), String> {
    if state.processing_session.pending() != 0 {
        app.emit("retry-recording-postprocessing", true)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub fn automatic_recording_claim_preparation(
    state: tauri::State<'_, Automation>,
    request: String,
) -> bool {
    if !state.ready.load(std::sync::atomic::Ordering::SeqCst)
        || state.processing_session.pending() != 0
        || recording::current_session().is_some()
    {
        return false;
    }
    let generation = state.generation.load(std::sync::atomic::Ordering::SeqCst);
    state
        .preparation
        .lock()
        .unwrap()
        .as_mut()
        .map(|(lease, _)| lease.claim(&request, generation, Instant::now()))
        .unwrap_or(false)
}
#[tauri::command]
pub fn automatic_recording_cancel_preparation(
    state: tauri::State<'_, Automation>,
    request: String,
) {
    let mut pending = state.preparation.lock().unwrap();
    if pending
        .as_ref()
        .map(|(lease, _)| lease.request == request)
        .unwrap_or(false)
    {
        pending.take();
    }
}
#[tauri::command]
pub fn automatic_recording_prepared(state: tauri::State<'_, Automation>, request: String) {
    let mut pending = state.preparation.lock().unwrap();
    if pending
        .as_ref()
        .map(|(lease, _)| lease.request == request && lease.claimed)
        .unwrap_or(false)
    {
        if let Some((_, sender)) = pending.take() {
            let _ = sender.send(());
        }
    }
}
#[tauri::command]
pub fn open_automatic_recording_accessibility() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Automatic call detection is available on macOS only.".into())
    }
}
fn label(o: Observation) -> String {
    format!("{o:?}").to_lowercase()
}
pub fn start<R: Runtime>(app: AppHandle<R>) {
    let state = Automation::default();
    match app.store("automatic_recording.json") {
        Ok(store) => {
            if let Some(value) = store.get("preferences") {
                match serde_json::from_value(value) {
                    Ok(prefs) => state.status.lock().unwrap().preferences = prefs,
                    Err(e) => {
                        state.status.lock().unwrap().error = Some(format!(
                            "Could not load automatic recording preferences: {e}"
                        ))
                    }
                }
            }
        }
        Err(e) => {
            state.status.lock().unwrap().error = Some(format!(
                "Could not load automatic recording preferences: {e}"
            ))
        }
    }
    app.manage(state);
    tauri::async_runtime::spawn(async move {
        let origin = Instant::now();
        let mut policy = Policy::default();
        let mut next_stop_retry = Instant::now();
        let mut last_generation = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let state = app.state::<Automation>();
            if state
                .relinquish
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                policy = Policy::default();
            }
            let (prefs, generation) = {
                let status = state.status.lock().unwrap();
                (
                    status.preferences.clone(),
                    state.generation.load(std::sync::atomic::Ordering::SeqCst),
                )
            };
            if generation != last_generation {
                if policy.owned.is_none() {
                    policy = Policy::default();
                } else {
                    policy.reset_confirmation();
                }
                last_generation = generation;
            }
            let enabled = prefs.teams || prefs.zoom;
            // Do not inspect other applications until the user opts in.
            let (permission, observations) = if enabled && cfg!(target_os = "macos") {
                tokio::task::spawn_blocking(native::observe)
                    .await
                    .unwrap_or((false, [Observation::Unknown; 2]))
            } else {
                (false, [Observation::Unknown; 2])
            };
            {
                let mut status = state.status.lock().unwrap();
                status.accessibility = permission;
                status.waiting_for_save = state.processing_session.pending() != 0;
                status.teams = label(observations[0]);
                status.zoom = label(observations[1]);
                let _ = app.emit("automatic-recording-status", status.clone());
            }
            if state.status.lock().unwrap().preferences != prefs {
                policy.tick(
                    origin.elapsed().as_secs(),
                    true,
                    Observation::Unknown,
                    recording::current_session(),
                );
                continue;
            }
            let mut selected = Vec::new();
            if prefs.teams {
                selected.push(observations[0]);
            }
            if prefs.zoom {
                selected.push(observations[1]);
            }
            let observation = policy::combined(&selected);
            // During a stop failure, wait before retrying rather than hammering audio.
            if Instant::now() < next_stop_retry && policy.owned.is_some() && enabled {
                continue;
            }
            if policy.owned.is_none()
                && (!state.ready.load(std::sync::atomic::Ordering::SeqCst)
                    || state.processing_session.pending() != 0)
            {
                continue;
            }
            let action = policy.tick(
                origin.elapsed().as_secs(),
                enabled,
                observation,
                recording::current_session(),
            );
            let result = match action {
                Some(Action::Start) => {
                    let result = start_owned(&app, generation).await;
                    policy.started(result.as_ref().ok().copied());
                    if state.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
                        policy = Policy::default();
                    }
                    result.map(|_| ())
                }
                Some(Action::Stop(id)) => {
                    let _lock = recording::LIFECYCLE.lock().await;
                    if state
                        .relinquish
                        .swap(false, std::sync::atomic::Ordering::SeqCst)
                    {
                        policy = Policy::default();
                        continue;
                    }
                    let matches_session = recording::current_session() == Some(id);
                    let result = if matches_session {
                        recording::stop_recording_locked(
                            app.clone(),
                            recording::RecordingArgs {
                                save_path: String::new(),
                            },
                        )
                        .await
                    } else {
                        Ok(())
                    };
                    policy.stopped(result.is_ok());
                    if result.is_ok() && matches_session {
                        crate::tray::update_tray_menu(&app);
                        let _ = app.emit("recording-stop-complete", true);
                    } else {
                        next_stop_retry = Instant::now() + Duration::from_secs(30);
                    }
                    result
                }
                None => continue,
            };
            let mut status = state.status.lock().unwrap();
            status.error = result.err().map(|e| format!("Automatic recording: {e}"));
            let _ = app.emit("automatic-recording-status", status.clone());
        }
    });
}
async fn start_owned<R: Runtime>(app: &AppHandle<R>, generation: u64) -> Result<u64, String> {
    let state = app.state::<Automation>();
    if !state.ready.load(std::sync::atomic::Ordering::SeqCst)
        || state.processing_session.pending() != 0
    {
        return Err("Open Meetily and complete setup before automatic recording can start.".into());
    }
    if !crate::onboarding::get_onboarding_status(app.clone())
        .await?
        .map(|s| s.completed)
        .unwrap_or(false)
    {
        return Err("Complete setup before automatic recording can start.".into());
    }
    // Serialize preparation and capture against every manual start and stop.
    let _lock = recording::LIFECYCLE.lock().await;
    if recording::current_session().is_some() {
        return Err("A recording is already in progress.".into());
    }
    let request = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    *state.preparation.lock().unwrap() = Some((
        preparation::Lease::new(request.clone(), generation, Instant::now()),
        sender,
    ));
    if let Err(error) = app.emit("automatic-recording-prepare", request) {
        state.preparation.lock().unwrap().take();
        return Err(error.to_string());
    }
    let mut prepared = tokio::time::timeout(Duration::from_secs(5), &mut receiver).await;
    if prepared.is_err() {
        // A frontend claim reserves this lifecycle. Do not release it underneath
        // a suspended webview that may still commit its synchronous preparation.
        let claimed = state
            .preparation
            .lock()
            .unwrap()
            .as_ref()
            .map(|(lease, _)| lease.claimed)
            .unwrap_or(false);
        if claimed {
            prepared = Ok(receiver.await);
        }
    }
    state.preparation.lock().unwrap().take();
    prepared
        .map_err(|_| "Frontend preparation timed out".to_string())?
        .map_err(|_| "Frontend preparation was cancelled".to_string())?;
    let prefs = recording_preferences::load_recording_preferences(app)
        .await
        .map_err(|e| e.to_string())?;
    let enabled = {
        let status = state.status.lock().unwrap();
        status.preferences.teams || status.preferences.zoom
    };
    if !enabled
        || state.generation.load(std::sync::atomic::Ordering::SeqCst) != generation
        || state.processing_session.pending() != 0
    {
        return Err("Automatic recording was disabled before capture started".into());
    }
    let start_result = recording::start_recording_with_devices_and_meeting_locked(
        app.clone(),
        prefs.preferred_mic_device,
        prefs.preferred_system_device,
        Some("Automatic meeting".into()),
        Some(generation),
    )
    .await;
    if let Err(error) = start_result {
        let _ = app.emit("automatic-recording-start-failed", error.clone());
        return Err(error);
    }
    crate::tray::update_tray_menu(app);
    let manager = app.state::<crate::notifications::commands::NotificationManagerState<R>>();
    if let Err(e) = crate::notifications::commands::show_recording_started_notification(
        app,
        &manager,
        Some("Automatic meeting".into()),
    )
    .await
    {
        log::warn!("Automatic recording notification failed: {e}");
    }
    recording::current_session().ok_or_else(|| "Recording did not establish a session".into())
}
