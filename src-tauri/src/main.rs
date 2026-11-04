#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    // STT worker re-launch: all native engine code runs in this child
    // process, before any Tauri state exists (Handy's engine_supervisor
    // pattern, adopted MIT). See reply_voice::stt.
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    if std::env::args_os().any(|a| a == reply_voice::stt::WORKER_FLAG) {
        std::process::exit(reply_voice::stt::run_worker());
    }
    ai_chat_pipeline_lib::run()
}
