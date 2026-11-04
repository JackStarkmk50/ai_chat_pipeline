
//! Reply Voice core: config, local receiver, speech queue.
//! Only the approved reply text ever crosses the receiver. Nothing else is read or sent.

pub mod autostart;
pub mod config;
pub mod receiver;
pub mod service;
pub mod speaker;
pub mod status;
pub mod voice;
