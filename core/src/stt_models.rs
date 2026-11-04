//! Speech-to-text models for dictation: the approved English candidate list,
//! where models live on disk, and verified background downloads.
//!
//! Same security posture as voice.rs: https-only URLs, plain file names,
//! SHA-256 checked before first use, nothing fetched until the user presses
//! "Download". The list ships inside the app (stt-manifest.json); a file with
//! the same name next to the settings file overrides it. Metadata mirrors
//! Handy's catalog (schema 2): pinned Hugging Face repo + revision, per-quant
//! size and SHA-256, licence tag. CC-BY models carry an attribution string
//! that the notices surface must show.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::voice::{Download, sha256_file};

/// Dictation is built for 64-bit Windows and Linux (Handy's engine targets).
pub const LISTEN_SUPPORTED: bool = cfg!(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
));

const EMBEDDED_MANIFEST: &str = include_str!("../../stt-manifest.json");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub label: String,
    pub description: String,
    pub url: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub license: String,
    #[serde(default)]
    pub attribution: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SttManifest {
    pub schema: u32,
    /// transcribe-cpp version the entries were verified against (Handy catalog).
    pub engine: String,
    pub models: Vec<ModelEntry>,
}

impl SttManifest {
    /// Same checks as voice.rs Manifest::validate, plus unique ids and the
    /// CC-BY attribution requirement.
    pub fn validate(&self) -> Result<(), String> {
        if self.models.is_empty() {
            return Err("The speech model list is empty".into());
        }
        let plain = |n: &str| {
            !n.is_empty() && !n.contains('/') && !n.contains('\\') && n != ".." && n != "."
        };
        for (i, m) in self.models.iter().enumerate() {
            if self.models.iter().skip(i + 1).any(|o| o.id == m.id) {
                return Err(format!("Duplicate model id {}", m.id));
            }
            if m.license.starts_with("cc-by") && m.attribution.is_none() {
                return Err(format!("Model {} needs an attribution string", m.id));
            }
            if !m.url.starts_with("https://") {
                return Err("Downloads must use https".into());
            }
            if !plain(&m.file) || m.sha256.len() != 64 || !m.sha256.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Bad file name or checksum in the speech model list".into());
            }
        }
        Ok(())
    }
}

pub fn stt_dir() -> PathBuf {
    crate::config::default_path()
        .parent()
        .map(|p| p.join("stt"))
        .unwrap_or_else(|| PathBuf::from("stt"))
}

/// Optional owner override next to the settings file.
pub fn manifest_path() -> PathBuf {
    crate::config::default_path().with_file_name("stt-manifest.json")
}

fn parse_manifest(text: &str) -> Option<SttManifest> {
    let m: SttManifest = serde_json::from_str(text).ok()?;
    m.validate().ok()?;
    Some(m)
}

/// The override file wins when present and valid; otherwise the shipped list.
pub fn load_manifest() -> Option<SttManifest> {
    if let Ok(text) = std::fs::read_to_string(manifest_path()) {
        if let Some(m) = parse_manifest(&text) {
            return Some(m);
        }
    }
    parse_manifest(EMBEDDED_MANIFEST)
}

pub fn model_path(base: &Path, e: &ModelEntry) -> PathBuf {
    base.join(&e.file)
}

pub fn installed(base: &Path, e: &ModelEntry) -> bool {
    model_path(base, e).is_file()
}

/// The requested model when installed; with no request, the first installed
/// entry in list order. None when nothing is installed.
pub fn pick(base: &Path, m: &SttManifest, want: &str) -> Option<ModelEntry> {
    if !want.is_empty() {
        if let Some(e) = m.models.iter().find(|e| e.id == want && installed(base, e)) {
            return Some(e.clone());
        }
    }
    m.models.iter().find(|e| installed(base, e)).cloned()
}

/// Background model download: .part file, SHA-256 check, rename into place.
/// Mirrors voice.rs Installer without the voice-program unpack step.
#[derive(Clone)]
pub struct SttInstaller {
    pub state: Arc<Mutex<Download>>,
    cancel: Arc<AtomicBool>,
}

impl Default for SttInstaller {
    fn default() -> Self {
        SttInstaller {
            state: Arc::new(Mutex::new(Download::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl SttInstaller {
    pub fn get(&self) -> Download {
        self.state.lock().unwrap().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Download and verify in the background. Does nothing if already running.
    pub fn start(&self, base: PathBuf, entry: ModelEntry) {
        {
            let mut s = self.state.lock().unwrap();
            if matches!(*s, Download::Downloading { .. }) {
                return;
            }
            *s = Download::Downloading {
                done: 0,
                total: entry.size,
            };
        }
        self.cancel.store(false, Ordering::SeqCst);
        let me = self.clone();
        std::thread::spawn(move || {
            *me.state.lock().unwrap() = match me.run(&base, &entry) {
                Ok(()) => Download::Done,
                Err(e) => Download::Failed(e),
            };
        });
    }

    fn run(&self, base: &Path, entry: &ModelEntry) -> Result<(), String> {
        std::fs::create_dir_all(base).map_err(|e| format!("Could not make the model folder: {e}"))?;
        let part = base.join(format!("{}.part", entry.file));
        let _ = std::fs::remove_file(&part);
        let mut child = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" })
            .args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "-o"])
            .arg(&part)
            .arg(&entry.url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not start the download (curl was not found)".to_string())?;
        let status = loop {
            if self.cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&part);
                return Err("Cancelled".into());
            }
            match child.try_wait() {
                Ok(Some(st)) => break st,
                Ok(None) => {}
                Err(e) => return Err(e.to_string()),
            }
            let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
            *self.state.lock().unwrap() = Download::Downloading {
                done: got.min(entry.size),
                total: entry.size,
            };
            std::thread::sleep(Duration::from_millis(150));
        };
        if !status.success() {
            let _ = std::fs::remove_file(&part);
            return Err("The download failed. Check the connection and try again.".into());
        }
        let got = sha256_file(&part).map_err(|e| e.to_string())?;
        if !got.eq_ignore_ascii_case(&entry.sha256) {
            let _ = std::fs::remove_file(&part);
            return Err(
                "The downloaded file did not match the expected checksum, so it was thrown away."
                    .into(),
            );
        }
        std::fs::rename(&part, model_path(base, entry)).map_err(|e| e.to_string())?;
        Ok(())
    }
}
