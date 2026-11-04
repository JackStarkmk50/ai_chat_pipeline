//! Tauri shell for Reply Voice. All real work lives in the `reply_voice` core crate;
//! this layer only exposes it to the React UI through commands and a state event.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use reply_voice::config::{self, Config};
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
use reply_voice::listen_service::{ListenService, Phase as ListenPhase};
use reply_voice::service::{Service, VoiceKind};
use reply_voice::status::Bridge;
use reply_voice::stt_models::{self, SttInstaller, SttManifest};
use reply_voice::voice::Download;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

struct AppState {
    service: Mutex<Service>,
    saved: Mutex<Config>,
    config_path: PathBuf,
    stt_manifest: Option<SttManifest>,
    stt_installer: SttInstaller,
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    listen: Mutex<ListenService>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Draft {
    voice: String,
    speed: f32,
    start_with_computer: bool,
    address: String,
    token: String,
    stt_model: String,
    first_run_done: bool,
}

impl From<Config> for Draft {
    fn from(c: Config) -> Draft {
        Draft {
            voice: c.voice,
            speed: c.speed,
            start_with_computer: c.start_with_computer,
            address: c.address,
            token: c.token,
            stt_model: c.stt_model,
            first_run_done: c.first_run_done,
        }
    }
}

impl From<Draft> for Config {
    fn from(d: Draft) -> Config {
        Config {
            voice: d.voice,
            speed: d.speed,
            start_with_computer: d.start_with_computer,
            address: d.address,
            token: d.token,
            stt_model: d.stt_model,
            first_run_done: d.first_run_done,
        }
    }
}

#[derive(Serialize, Clone, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
enum DownloadUi {
    Idle,
    Downloading { done: u64, total: u64 },
    Failed { message: String },
    Done,
}

#[derive(Serialize, Clone, PartialEq)]
struct UiState {
    bridge: String,
    link_error: Option<String>,
    speaking: bool,
    voice_kind: String,
    voice_name: String,
    voice_ready: bool,
    has_manifest: bool,
    manifest_mb: Option<u64>,
    download: DownloadUi,
    can_replay: bool,
    first_run_done: bool,
    last_error: Option<String>,
    listen_supported: bool,
    listen_phase: String,
    listen_pending: u32,
    transcript: Vec<String>,
    listen_error: Option<String>,
    stt_model_label: String,
    stt_model_ready: bool,
    stt_has_manifest: bool,
    stt_download: DownloadUi,
    stt_models: Vec<SttModelUi>,
}

#[derive(Serialize, Clone, PartialEq)]
struct SttModelUi {
    id: String,
    label: String,
    description: String,
    mb: u64,
    installed: bool,
    license: String,
}

fn stt_models_ui(manifest: &Option<SttManifest>) -> Vec<SttModelUi> {
    let base = stt_models::stt_dir();
    manifest
        .as_ref()
        .map(|m| {
            m.models
                .iter()
                .map(|e| SttModelUi {
                    id: e.id.clone(),
                    label: e.label.clone(),
                    description: e.description.clone(),
                    mb: e.size / 1_000_000,
                    installed: stt_models::installed(&base, e),
                    license: e.license.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ui_state(app: &AppState) -> UiState {
    let cfg = app.saved.lock().unwrap();
    let svc = app.service.lock().unwrap();
    let (listen_phase, listen_pending, transcript, listen_error, stt_model_label, stt_model_ready) =
        listen_bits(app);
    let stt_download = match app.stt_installer.get() {
        Download::Idle => DownloadUi::Idle,
        Download::Downloading { done, total } => DownloadUi::Downloading { done, total },
        Download::Failed(m) => DownloadUi::Failed { message: m },
        Download::Done => DownloadUi::Done,
    };
    let svc = &*svc;
    let cfg = &*cfg;
    let snap = svc.shared.get();
    let (bridge, link_error) = match &snap.bridge {
        Bridge::NotConnected => ("not_connected", None),
        Bridge::Listening => ("waiting", None),
        Bridge::Connected => ("received", None),
        Bridge::Failed(e) => ("not_connected", Some(e.clone())),
    };
    let download = match svc.installer.get() {
        Download::Idle => DownloadUi::Idle,
        Download::Downloading { done, total } => DownloadUi::Downloading { done, total },
        Download::Failed(m) => DownloadUi::Failed { message: m },
        Download::Done => DownloadUi::Done,
    };
    UiState {
        bridge: bridge.to_string(),
        link_error,
        speaking: snap.speaking,
        voice_kind: match svc.voice {
            VoiceKind::Piper => "piper",
            VoiceKind::System => "system",
            VoiceKind::None => "none",
        }
        .to_string(),
        voice_name: svc.voice_name.clone(),
        voice_ready: svc.voice_ready(),
        has_manifest: svc.manifest.is_some(),
        manifest_mb: svc.manifest.as_ref().map(|m| m.total_bytes() / 1_000_000),
        download,
        can_replay: snap.last_reply.is_some(),
        first_run_done: cfg.first_run_done,
        last_error: snap.last_error.clone(),
        listen_supported: stt_models::LISTEN_SUPPORTED,
        listen_phase,
        listen_pending,
        transcript,
        listen_error,
        stt_model_label,
        stt_model_ready,
        stt_has_manifest: app.stt_manifest.is_some(),
        stt_download,
        stt_models: stt_models_ui(&app.stt_manifest),
    }
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn listen_bits(
    app: &AppState,
) -> (String, u32, Vec<String>, Option<String>, String, bool) {
    let l = app.listen.lock().unwrap();
    let s = l.shared.get();
    let phase = match s.phase {
        ListenPhase::Idle if l.active() => "recording",
        ListenPhase::Idle => "idle",
        ListenPhase::Recording => "recording",
        ListenPhase::Transcribing => "transcribing",
    };
    (
        phase.to_string(),
        s.pending as u32,
        s.lines,
        s.last_error,
        l.model_label.clone(),
        l.model_ready,
    )
}

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
fn listen_bits(
    _app: &AppState,
) -> (String, u32, Vec<String>, Option<String>, String, bool) {
    ("idle".into(), 0, Vec::new(), None, String::new(), false)
}

#[tauri::command]
fn get_state(state: State<AppState>) -> UiState {
    ui_state(state.inner())
}

#[tauri::command]
fn get_draft(state: State<AppState>) -> Draft {
    state.saved.lock().unwrap().clone().into()
}

#[tauri::command]
fn validate_settings(draft: Draft) -> Option<String> {
    let mut cfg: Config = draft.into();
    cfg.clamp();
    cfg.validate().err().map(|e| e.to_string())
}

#[tauri::command]
fn save_settings(state: State<AppState>, draft: Draft) -> Option<String> {
    let mut cfg: Config = draft.into();
    cfg.clamp();
    if let Err(e) = cfg.validate() {
        return Some(e.to_string());
    }
    if let Err(e) = cfg.save(&state.config_path) {
        return Some(format!("Could not save: {e}"));
    }
    let mut saved = state.saved.lock().unwrap();
    if cfg.start_with_computer != saved.start_with_computer {
        if let Err(e) = reply_voice::autostart::set(cfg.start_with_computer) {
            return Some(e);
        }
    }
    *saved = cfg.clone();
    drop(saved);
    state.service.lock().unwrap().apply(&cfg);
    refresh_listen_model(state.inner());
    None
}

#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn refresh_listen_model(app: &AppState) {
    let want = app.saved.lock().unwrap().stt_model.clone();
    app.listen.lock().unwrap().refresh_model(&want);
}

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
fn refresh_listen_model(_app: &AppState) {}

#[tauri::command]
fn listen_start(state: State<AppState>) -> Option<String> {
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        let want = state.saved.lock().unwrap().stt_model.clone();
        return state.listen.lock().unwrap().start(&want).err();
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    {
        let _ = &state;
        Some("Dictation is not supported on this computer".into())
    }
}

#[tauri::command]
fn listen_stop(state: State<AppState>) {
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    state.listen.lock().unwrap().stop();
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    let _ = &state;
}

#[tauri::command]
fn download_stt_model(state: State<AppState>, id: String) -> Option<String> {
    let Some(manifest) = &state.stt_manifest else {
        return Some("No speech model list is set up".into());
    };
    let Some(entry) = manifest.models.iter().find(|e| e.id == id) else {
        return Some("That speech model is not in the list".into());
    };
    state
        .stt_installer
        .start(stt_models::stt_dir(), entry.clone());
    None
}

#[tauri::command]
fn cancel_stt_download(state: State<AppState>) {
    state.stt_installer.cancel();
}

#[tauri::command]
fn make_token() -> String {
    config::generate_token()
}

#[tauri::command]
fn play_sample(state: State<AppState>) {
    let speed = state.saved.lock().unwrap().speed;
    state.service.lock().unwrap().play_sample(speed);
}

#[tauri::command]
fn download_voice(state: State<AppState>) -> Option<String> {
    let svc = state.service.lock().unwrap();
    match &svc.manifest {
        Some(m) => {
            svc.installer.start(svc.base.clone(), m.clone());
            None
        }
        None => Some("No voice list is set up".into()),
    }
}

#[tauri::command]
fn cancel_download(state: State<AppState>) {
    state.service.lock().unwrap().installer.cancel();
}

#[tauri::command]
fn replay_last(state: State<AppState>) -> Option<String> {
    state.service.lock().unwrap().replay().err()
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

/// The tray icon: a plain orange circle, same as the previous build.
fn tray_icon() -> tauri::image::Image<'static> {
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32i32 {
        for x in 0..32i32 {
            let (dx, dy) = (x - 16, y - 16);
            let inside = dx * dx + dy * dy <= 15 * 15;
            rgba.extend_from_slice(if inside {
                &[0xC2, 0x41, 0x0C, 0xFF]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    tauri::image::Image::new_owned(rgba, 32, 32)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_draft,
            validate_settings,
            save_settings,
            make_token,
            play_sample,
            download_voice,
            cancel_download,
            replay_last,
            hide_window,
            listen_start,
            listen_stop,
            download_stt_model,
            cancel_stt_download
        ])
        .setup(|app| {
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;

            let config_path = config::default_path();
            let cfg = Config::load(&config_path);
            let mut svc = Service::new();
            svc.apply(&cfg);
            app.manage(AppState {
                service: Mutex::new(svc),
                saved: Mutex::new(cfg),
                config_path,
                stt_manifest: stt_models::load_manifest(),
                stt_installer: SttInstaller::default(),
                #[cfg(any(
                    all(target_os = "windows", target_arch = "x86_64"),
                    all(target_os = "linux", target_arch = "x86_64")
                ))]
                listen: Mutex::new(ListenService::new()),
            });
            refresh_listen_model(app.state::<AppState>().inner());

            // Push UI state whenever anything changes (reply arrives, download
            // progress, speech starts or stops).
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last: Option<UiState> = None;
                loop {
                    std::thread::sleep(Duration::from_millis(400));
                    let st = handle.state::<AppState>();
                    {
                        let mut svc = st.service.lock().unwrap();
                        if svc.installer.get() == Download::Done {
                            *svc.installer.state.lock().unwrap() = Download::Idle;
                            svc.refresh_voice();
                        }
                    }
                    if st.stt_installer.get() == Download::Done {
                        *st.stt_installer.state.lock().unwrap() = Download::Idle;
                        refresh_listen_model(st.inner());
                    }
                    let ui = ui_state(st.inner());
                    if last.as_ref() != Some(&ui) {
                        let _ = handle.emit("rv-state", ui.clone());
                        last = Some(ui);
                    }
                }
            });

            let open = MenuItem::with_id(app, "open", "Open Reply Voice", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Reply Voice")
                .icon(tray_icon())
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => std::process::exit(0),
                    _ => {}
                })
                .build(app)?;
            // The tray icon must live as long as the app.
            std::mem::forget(_tray);

            // Closing the window keeps Reply Voice running in the tray.
            if let Some(w) = app.get_webview_window("main") {
                let w2 = w.clone();
                w.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let _ = w2.hide();
                        api.prevent_close();
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Reply Voice");
}
