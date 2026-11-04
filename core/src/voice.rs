//! A local neural voice, run through the Piper program. No model is chosen or bundled: the voice
//! to download comes from a manifest file the owner provides (see the README). Nothing is fetched
//! until the user presses "Download voice", and every file is checked against the manifest's
//! SHA-256 before it is used.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::speaker::Engine;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub file: String,
    pub extract: bool,
}

/// What to download, and which file is the voice model. Provided by the owner, never built in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub model_file: String,
    pub items: Vec<Item>,
}

impl Manifest {
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.size).sum()
    }

    /// Checks the manifest is safe to use: https only, plain file names, real checksums.
    pub fn validate(&self) -> Result<(), String> {
        if self.items.is_empty() || self.model_file.is_empty() {
            return Err("The voice list is empty".into());
        }
        let plain = |n: &str| {
            !n.is_empty() && !n.contains('/') && !n.contains('\\') && n != ".." && n != "."
        };
        if !plain(&self.model_file) {
            return Err("Bad model file name".into());
        }
        for i in &self.items {
            if !(i.url.starts_with("https://") || i.url.starts_with("file://")) {
                return Err("Downloads must use https".into());
            }
            if !plain(&i.file)
                || i.sha256.len() != 64
                || !i.sha256.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err("Bad file name or checksum in the voice list".into());
            }
        }
        if !self.items.iter().any(|i| i.file == self.model_file) {
            return Err("The model file is not in the download list".into());
        }
        Ok(())
    }
}

pub fn manifest_path() -> PathBuf {
    crate::config::default_path().with_file_name("voice-manifest.json")
}

pub fn load_manifest(path: &Path) -> Option<Manifest> {
    let m: Manifest = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    m.validate().ok()?;
    Some(m)
}

pub fn piper_exe(base: &Path) -> PathBuf {
    base.join("piper")
        .join(if cfg!(windows) { "piper.exe" } else { "piper" })
}

/// The manifest that was installed, if the voice is fully installed.
pub fn installed_manifest(base: &Path) -> Option<Manifest> {
    let m = load_manifest(&base.join("installed.json"))?;
    if piper_exe(base).is_file() && base.join(&m.model_file).is_file() {
        Some(m)
    } else {
        None
    }
}

pub fn installed(base: &Path) -> bool {
    installed_manifest(base).is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Download {
    Idle,
    Downloading { done: u64, total: u64 },
    Failed(String),
    Done,
}

#[derive(Clone)]
pub struct Installer {
    pub state: Arc<Mutex<Download>>,
    cancel: Arc<AtomicBool>,
}

impl Default for Installer {
    fn default() -> Self {
        Installer {
            state: Arc::new(Mutex::new(Download::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Installer {
    pub fn get(&self) -> Download {
        self.state.lock().unwrap().clone()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// Download and install in the background. Does nothing if already running.
    pub fn start(&self, base: PathBuf, manifest: Manifest) {
        let items = manifest.items.clone();
        {
            let mut s = self.state.lock().unwrap();
            if matches!(*s, Download::Downloading { .. }) {
                return;
            }
            *s = Download::Downloading {
                done: 0,
                total: items.iter().map(|i| i.size).sum(),
            };
        }
        self.cancel.store(false, Ordering::SeqCst);
        let me = self.clone();
        std::thread::spawn(move || {
            let res = me.run(&base, &items).and_then(|_| {
                std::fs::write(
                    base.join("installed.json"),
                    serde_json::to_vec_pretty(&manifest).unwrap(),
                )
                .map_err(|e| e.to_string())
            });
            *me.state.lock().unwrap() = match res {
                Ok(()) => Download::Done,
                Err(e) => Download::Failed(e),
            };
        });
    }

    fn run(&self, base: &Path, items: &[Item]) -> Result<(), String> {
        std::fs::create_dir_all(base)
            .map_err(|e| format!("Could not make the voice folder: {e}"))?;
        let _ = std::fs::remove_file(base.join("installed.json"));
        let total: u64 = items.iter().map(|i| i.size).sum();
        let mut before = 0u64;
        for it in items {
            let part = base.join(format!("{}.part", it.file));
            let _ = std::fs::remove_file(&part);
            let mut child = Command::new(curl_name())
                .args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "-o"])
                .arg(&part)
                .arg(&it.url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
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
                    done: (before + got.min(it.size)).min(total),
                    total,
                };
                std::thread::sleep(Duration::from_millis(150));
            };
            if !status.success() {
                let _ = std::fs::remove_file(&part);
                return Err("The download failed. Check the connection and try again.".into());
            }
            let got = sha256_file(&part).map_err(|e| e.to_string())?;
            if !got.eq_ignore_ascii_case(&it.sha256) {
                let _ = std::fs::remove_file(&part);
                return Err("The downloaded file did not match the expected checksum, so it was thrown away.".into());
            }
            let dest = base.join(&it.file);
            std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
            if it.extract {
                let out = Command::new("tar")
                    .arg("-xf")
                    .arg(&dest)
                    .arg("-C")
                    .arg(base)
                    .stdin(Stdio::null())
                    .output()
                    .map_err(|_| "Could not unpack (tar was not found)".to_string())?;
                let _ = std::fs::remove_file(&dest);
                if !out.status.success() {
                    return Err("Could not unpack the voice program".into());
                }
            }
            before += it.size;
        }
        if !piper_exe(base).is_file() {
            return Err("The voice program was not where expected after unpacking".into());
        }
        Ok(())
    }
}

fn curl_name() -> &'static str {
    if cfg!(windows) { "curl.exe" } else { "curl" }
}

pub fn sha256_file(p: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Piper length_scale: bigger is slower, so invert the speed.
pub fn length_scale(speed: f32) -> f32 {
    (1.0 / speed.clamp(0.5, 2.0) * 100.0).round() / 100.0
}

#[cfg(windows)]
fn player() -> Option<(String, Vec<String>)> {
    Some((
        "powershell.exe".into(),
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-WindowStyle".into(),
            "Hidden".into(),
            "-Command".into(),
            "(New-Object System.Media.SoundPlayer $env:RV_WAV).PlaySync()".into(),
        ],
    ))
}

#[cfg(not(windows))]
fn player() -> Option<(String, Vec<String>)> {
    let paths = std::env::var_os("PATH")?;
    let dirs: Vec<_> = std::env::split_paths(&paths).collect();
    for (name, args) in [
        ("paplay", vec![]),
        ("aplay", vec!["-q"]),
        ("ffplay", vec!["-nodisp", "-autoexit", "-loglevel", "quiet"]),
        ("play", vec!["-q"]),
    ] {
        if dirs.iter().any(|d| d.join(name).is_file()) {
            return Some((name.into(), args.into_iter().map(String::from).collect()));
        }
    }
    None
}

pub fn can_play() -> bool {
    player().is_some()
}

pub struct PiperVoice {
    base: PathBuf,
    model: String,
    pub name: String,
    child: Arc<Mutex<Option<Child>>>,
}

impl PiperVoice {
    pub fn open(base: &Path) -> Option<PiperVoice> {
        if let (Some(m), true) = (installed_manifest(base), can_play()) {
            Some(PiperVoice {
                base: base.to_path_buf(),
                model: m.model_file,
                name: m.name,
                child: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        }
    }

    fn run(&self, mut cmd: Command, input: Option<&str>) -> Result<(), String> {
        use std::io::Write;
        cmd.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start: {e}"))?;
        if let (Some(t), Some(mut si)) = (input, child.stdin.take()) {
            si.write_all(t.as_bytes()).map_err(|e| e.to_string())?;
        }
        *self.child.lock().unwrap() = Some(child);
        loop {
            let mut g = self.child.lock().unwrap();
            match g.as_mut() {
                None => return Err("Stopped".into()),
                Some(c) => match c.try_wait() {
                    Ok(Some(st)) => {
                        *g = None;
                        return if st.success() {
                            Ok(())
                        } else {
                            Err("The voice stopped with an error".into())
                        };
                    }
                    Ok(None) => {}
                    Err(e) => return Err(e.to_string()),
                },
            }
            drop(g);
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}

impl Engine for PiperVoice {
    fn speak(&mut self, text: &str, speed: f32) -> Result<(), String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let wav = std::env::temp_dir().join(format!(
            "reply-voice-{}-{}.wav",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let mut synth = Command::new(piper_exe(&self.base));
        synth
            .arg("--model")
            .arg(self.base.join(&self.model))
            .arg("--output_file")
            .arg(&wav)
            .arg("--length_scale")
            .arg(length_scale(speed).to_string());
        let res = self.run(synth, Some(text)).and_then(|_| {
            let (prog, args) = player().ok_or("No audio player found")?;
            let mut play = Command::new(prog);
            play.args(args);
            if cfg!(windows) {
                play.env("RV_WAV", &wav);
            } else {
                play.arg(&wav);
            }
            self.run(play, None)
        });
        let _ = std::fs::remove_file(&wav);
        res
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
