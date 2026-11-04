use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::receiver::{self, Handle};
use crate::speaker::{Engine, Speaker, SystemVoice};
use crate::status::{Bridge, Shared};
use crate::voice::{Installer, Manifest, PiperVoice};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VoiceKind {
    /// The downloaded local neural voice. The main voice.
    Piper,
    /// The voice already on the computer. A labelled fallback only.
    System,
    None,
}

pub fn voice_dir() -> PathBuf {
    crate::config::default_path()
        .parent()
        .map(|p| p.join("voice"))
        .unwrap_or_else(|| PathBuf::from("voice"))
}

/// Pick the voice: downloaded neural voice first, system voice as fallback.
pub fn open_engine(base: &std::path::Path) -> (VoiceKind, Option<Box<dyn Engine>>) {
    if let Some(p) = PiperVoice::open(base) {
        return (VoiceKind::Piper, Some(Box::new(p)));
    }
    if let Some(s) = SystemVoice::detect() {
        return (VoiceKind::System, Some(Box::new(s)));
    }
    (VoiceKind::None, None)
}

/// Owns the speaker and the receiver. Restarted whenever Settings are saved.
pub struct Service {
    pub shared: Shared,
    pub voice: VoiceKind,
    pub installer: Installer,
    pub manifest: Option<Manifest>,
    pub voice_name: String,
    pub base: PathBuf,
    speaker: Option<Arc<Speaker>>,
    handle: Option<Handle>,
}

impl Service {
    pub fn new() -> Service {
        Service::with_dir(voice_dir())
    }

    pub fn with_dir(base: PathBuf) -> Service {
        let voice = open_engine(&base).0;
        let manifest = crate::voice::load_manifest(&crate::voice::manifest_path());
        let voice_name = crate::voice::installed_manifest(&base)
            .map(|m| m.name)
            .unwrap_or_default();
        Service {
            shared: Shared::default(),
            voice,
            installer: Installer::default(),
            manifest,
            voice_name,
            base,
            speaker: None,
            handle: None,
        }
    }

    pub fn refresh_voice(&mut self) {
        self.voice = open_engine(&self.base).0;
        self.voice_name = crate::voice::installed_manifest(&self.base)
            .map(|m| m.name)
            .unwrap_or_default();
    }

    /// True when some voice can speak right now.
    pub fn voice_ready(&self) -> bool {
        self.voice != VoiceKind::None
    }

    pub fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.stop();
        }
        self.speaker = None;
        self.shared.update(|s| {
            s.bridge = Bridge::NotConnected;
        });
    }

    /// Start with the saved settings. Does nothing if the link is not set up.
    pub fn apply(&mut self, cfg: &Config) {
        self.stop();
        let (kind, engine) = open_engine(&self.base);
        self.voice = kind;
        if !cfg.link_configured() {
            return;
        }
        let Some(engine) = engine else {
            self.shared
                .update(|s| s.bridge = Bridge::Failed("No voice yet".into()));
            return;
        };
        let addr = match cfg.socket_addr() {
            Ok(a) => a,
            Err(e) => {
                self.shared
                    .update(|s| s.bridge = Bridge::Failed(e.to_string()));
                return;
            }
        };
        let speaker = Arc::new(Speaker::start(engine, self.shared.clone(), cfg.speed));
        match receiver::start(
            addr,
            cfg.token.clone(),
            speaker.clone(),
            self.shared.clone(),
        ) {
            Ok(h) => {
                self.handle = Some(h);
                self.speaker = Some(speaker);
            }
            Err(e) => self.shared.update(|s| s.bridge = Bridge::Failed(e)),
        }
    }
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}
