use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

pub const MIN_SPEED: f32 = 0.75;
pub const MAX_SPEED: f32 = 1.5;
pub const DEFAULT_ADDRESS: &str = "127.0.0.1:47615";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// English only for now.
    pub voice: String,
    pub speed: f32,
    pub start_with_computer: bool,
    /// Address the local receiver listens on. Must be a loopback address.
    pub address: String,
    /// Shared secret the sender must present. Empty means the receiver stays off.
    pub token: String,
    /// Dictation speech model id from stt-manifest.json. Empty uses the first
    /// installed entry in list order.
    pub stt_model: String,
    pub first_run_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            voice: "English, default voice".into(),
            speed: 1.0,
            start_with_computer: false,
            address: DEFAULT_ADDRESS.into(),
            token: String::new(),
            stt_model: String::new(),
            first_run_done: false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    BadAddress,
    NotLoopback,
    TokenTooShort,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::BadAddress => write!(f, "Use an address like 127.0.0.1:47615"),
            ConfigError::NotLoopback => write!(f, "Only this computer (127.0.0.1) is allowed"),
            ConfigError::TokenTooShort => write!(f, "The token needs at least 16 characters"),
        }
    }
}

impl Config {
    pub fn clamp(&mut self) {
        if !self.speed.is_finite() {
            self.speed = 1.0;
        }
        self.speed = self.speed.clamp(MIN_SPEED, MAX_SPEED);
    }

    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        let addr: SocketAddr = self
            .address
            .trim()
            .parse()
            .map_err(|_| ConfigError::BadAddress)?;
        if !is_loopback(addr.ip()) {
            return Err(ConfigError::NotLoopback);
        }
        Ok(addr)
    }

    /// Check what the Settings screen is about to save.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.socket_addr()?;
        if !self.token.is_empty() && self.token.chars().count() < 16 {
            return Err(ConfigError::TokenTooShort);
        }
        Ok(())
    }

    /// The link is only usable with a valid address and a token.
    pub fn link_configured(&self) -> bool {
        !self.token.is_empty() && self.socket_addr().is_ok()
    }

    pub fn load(path: &Path) -> Config {
        let mut c: Config = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        c.clamp();
        c
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(tmp, path)
    }
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "ReplyVoice")
        .map(|d| d.config_dir().join("config.json"))
        .unwrap_or_else(|| PathBuf::from("reply-voice-config.json"))
}

/// A random token for the user to paste into the sender. Not secret-safe on its own,
/// uses OS randomness where available.
pub fn generate_token() -> String {
    let mut buf = [0u8; 24];
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut buf);
        }
    }
    #[cfg(not(unix))]
    {
        use std::hash::{BuildHasher, Hasher};
        for chunk in buf.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(chunk.len() as u64);
            let v = h.finish().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}
