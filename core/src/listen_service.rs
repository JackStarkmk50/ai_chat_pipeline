//! One explicit dictation: microphone + VAD session feeding a supervised
//! transcription worker process. Started and stopped by the user in the app;
//! nothing listens otherwise. Transcript lines stay in memory on this
//! computer - they are never sent anywhere.

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use crate::listen::{ListenSession, SpeechSegment};
use crate::stt::Supervisor;
use crate::stt_models;

/// Earshot operating point used in the validated pipeline tests.
const VAD_THRESHOLD: f32 = 0.5;
/// Small bounded queue: the transcriber must never stall the microphone.
const SEGMENT_QUEUE: usize = 4;
const MAX_LINES: usize = 100;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub phase: Phase,
    pub pending: usize,
    pub lines: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Snapshot>>);

impl Shared {
    pub fn get(&self) -> Snapshot {
        self.0.lock().unwrap().clone()
    }
    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        f(&mut self.0.lock().unwrap());
    }
}

/// Owns the current dictation, if any. The supervisor and the microphone are
/// only alive between Record and Stop.
pub struct ListenService {
    pub shared: Shared,
    pub model_label: String,
    pub model_ready: bool,
    session: Option<ListenSession>,
    drain: Option<std::thread::JoinHandle<()>>,
}

impl ListenService {
    pub fn new() -> ListenService {
        let mut s = ListenService {
            shared: Shared::default(),
            model_label: String::new(),
            model_ready: false,
            session: None,
            drain: None,
        };
        s.refresh_model("");
        s
    }

    pub fn active(&self) -> bool {
        self.session.is_some()
    }

    /// Re-detect the usable model after a download or a settings change.
    pub fn refresh_model(&mut self, want: &str) {
        let base = stt_models::stt_dir();
        let picked = stt_models::load_manifest()
            .and_then(|m| stt_models::pick(&base, &m, want))
            .map(|e| e.label);
        self.model_ready = picked.is_some();
        self.model_label = picked.unwrap_or_default();
    }

    /// Load the model in a worker process and start the microphone.
    /// Errors are returned, never hidden.
    pub fn start(&mut self, want: &str) -> Result<(), String> {
        if self.session.is_some() {
            return Ok(());
        }
        if let Some(d) = self.drain.take() {
            let _ = d.join();
        }
        let base = stt_models::stt_dir();
        let manifest = stt_models::load_manifest().ok_or("No speech model list is set up")?;
        let entry = stt_models::pick(&base, &manifest, want)
            .ok_or("No speech model on this computer yet. Download one in Settings.")?;
        let mut sup = Supervisor::new();
        sup.load(&stt_models::model_path(&base, &entry))
            .map_err(|e| e.to_string())?;
        let (tx, rx) = sync_channel::<SpeechSegment>(SEGMENT_QUEUE);
        let session = ListenSession::start(tx, VAD_THRESHOLD).map_err(|e| {
            sup.unload();
            e.0
        })?;
        let shared = self.shared.clone();
        shared.update(|s| {
            s.phase = Phase::Recording;
            s.pending = 0;
            s.last_error = None;
        });
        self.drain = Some(std::thread::spawn(move || {
            while let Ok(seg) = rx.recv() {
                shared.update(|s| s.pending += 1);
                match sup.transcribe(&seg.pcm) {
                    Ok(t) => {
                        let text = t.text.trim().to_string();
                        if !text.is_empty() {
                            shared.update(|s| {
                                s.lines.push(text);
                                if s.lines.len() > MAX_LINES {
                                    let cut = s.lines.len() - MAX_LINES;
                                    s.lines.drain(..cut);
                                }
                            });
                        }
                    }
                    Err(e) => shared.update(|s| s.last_error = Some(e.to_string())),
                }
                shared.update(|s| s.pending = s.pending.saturating_sub(1));
            }
            // Recording is over: free the model's CPU/GPU memory immediately.
            sup.unload();
            shared.update(|s| {
                s.phase = Phase::Idle;
                s.pending = 0;
            });
        }));
        self.session = Some(session);
        Ok(())
    }

    /// Stop recording. Segments already captured still finish transcribing;
    /// the phase shows Transcribing until they are done.
    pub fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            if let Err(e) = session.stop() {
                self.shared.update(|s| s.last_error = Some(e.0));
            }
            self.shared.update(|s| {
                s.phase = if s.pending > 0 {
                    Phase::Transcribing
                } else {
                    Phase::Idle
                };
            });
            if let Some(d) = self.drain.take() {
                std::thread::spawn(move || {
                    let _ = d.join();
                });
            }
        }
    }
}
