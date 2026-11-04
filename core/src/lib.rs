//! Reply Voice core: config, local receiver, speech queue, dictation.
//! Only the approved reply text ever crosses the receiver. Dictation is
//! explicit (record/stop in the app) and its transcript stays on this
//! computer. Nothing else is read or sent.

pub mod autostart;
pub mod config;
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub mod listen;
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub mod listen_service;
pub mod receiver;
pub mod service;
pub mod speaker;
pub mod status;
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub mod stt;
pub mod stt_models;
pub mod voice;
