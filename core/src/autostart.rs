
//! Start with the computer. Off unless the user turns it on in Settings.
use std::path::PathBuf;

#[cfg(windows)]
const NAME: &str = "ReplyVoice";

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut cmd = Command::new("reg");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        cmd.args(["add", key, "/v", NAME, "/t", "REG_SZ", "/d"])
            .arg(format!("\"{}\"", exe.display()))
            .arg("/f");
    } else {
        cmd.args(["delete", key, "/v", NAME, "/f"]);
    }
    cmd.creation_flags(0x08000000);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() || !enabled {
        Ok(())
    } else {
        Err("Could not change the startup setting".into())
    }
}

#[cfg(not(windows))]
pub fn set(enabled: bool) -> Result<(), String> {
    let path = desktop_path().ok_or("No home folder")?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = desktop_entry(&exe.to_string_lossy());
        std::fs::write(&path, body).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(windows))]
pub fn desktop_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart").join("reply-voice.desktop"))
}

#[cfg(windows)]
pub fn desktop_path() -> Option<PathBuf> {
    None
}

pub fn desktop_entry(exe: &str) -> String {
    let quoted = exe.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName=Reply Voice\nExec=\"{quoted}\"\nX-GNOME-Autostart-enabled=true\n"
    )
}
