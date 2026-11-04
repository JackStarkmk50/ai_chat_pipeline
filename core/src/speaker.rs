use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};

use crate::status::Shared;

pub trait Engine: Send {
    /// Read text aloud, return when done. `speed` is 0.75 to 1.5.
    fn speak(&mut self, text: &str, speed: f32) -> Result<(), String>;
    fn stop(&mut self);
}

/// Uses the voice already on the computer. No download. Text goes in on stdin,
/// never on the command line, so it can't be read as a flag or a script.
pub struct SystemVoice {
    child: Arc<Mutex<Option<Child>>>,
}

impl SystemVoice {
    /// Returns None if this computer has no usable voice program.
    pub fn detect() -> Option<SystemVoice> {
        if system_program().is_some() {
            Some(SystemVoice {
                child: Arc::new(Mutex::new(None)),
            })
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn system_program() -> Option<(String, Vec<String>)> {
    // Windows ships System.Speech. The text is read from stdin by the script.
    let script = "Add-Type -AssemblyName System.Speech; \
        $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
        $s.Rate = [int]$env:RV_RATE; \
        $t = [Console]::In.ReadToEnd(); $s.Speak($t)";
    Some((
        "powershell.exe".into(),
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-WindowStyle".into(),
            "Hidden".into(),
            "-Command".into(),
            script.into(),
        ],
    ))
}

#[cfg(not(windows))]
fn system_program() -> Option<(String, Vec<String>)> {
    for name in ["espeak-ng", "espeak"] {
        if on_path(name) {
            return Some((
                name.into(),
                vec!["-v".into(), "en".into(), "--stdin".into()],
            ));
        }
    }
    None
}

#[cfg(not(windows))]
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// Map 0.75..1.5 to the Windows -10..10 rate scale.
pub fn windows_rate(speed: f32) -> i32 {
    (((speed - 1.0) / 0.5) * 10.0).round().clamp(-10.0, 10.0) as i32
}

/// espeak words per minute, 175 is its default.
pub fn espeak_wpm(speed: f32) -> u32 {
    (175.0 * speed).round().clamp(80.0, 450.0) as u32
}

impl Engine for SystemVoice {
    fn speak(&mut self, text: &str, speed: f32) -> Result<(), String> {
        let (prog, mut args) = system_program().ok_or("No voice on this computer")?;
        let mut cmd_env: Vec<(String, String)> = vec![];
        if cfg!(windows) {
            cmd_env.push(("RV_RATE".into(), windows_rate(speed).to_string()));
        } else {
            args.push("-s".into());
            args.push(espeak_wpm(speed).to_string());
        }
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in cmd_env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // no console window
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Could not start the voice: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        *self.child.lock().unwrap() = Some(child);
        // Wait without holding the lock so stop() can kill it.
        loop {
            let mut g = self.child.lock().unwrap();
            match g.as_mut() {
                None => return Ok(()),
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
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub const MAX_QUEUE: usize = 16;

/// One worker reads replies one at a time, in order.
pub struct Speaker {
    tx: SyncSender<String>,
    pub speed: Arc<Mutex<f32>>,
}

impl Speaker {
    pub fn start(mut engine: Box<dyn Engine>, shared: Shared, speed: f32) -> Speaker {
        let (tx, rx): (SyncSender<String>, Receiver<String>) = sync_channel(MAX_QUEUE);
        let speed = Arc::new(Mutex::new(speed));
        let sp = speed.clone();
        std::thread::spawn(move || {
            while let Ok(text) = rx.recv() {
                shared.update(|s| s.speaking = true);
                let v = *sp.lock().unwrap();
                let res = engine.speak(&text, v);
                shared.update(|s| {
                    s.speaking = false;
                    s.last_error = res.err();
                });
            }
        });
        Speaker { tx, speed }
    }

    /// Err(()) means the queue is full.
    pub fn enqueue(&self, text: String) -> Result<(), ()> {
        match self.tx.try_send(text) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => Err(()),
        }
    }
}
