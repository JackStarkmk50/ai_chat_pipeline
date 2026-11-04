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
    // Packaged-runtime self-check (CI and on-device verification):
    // `<exe> --replyvoice-stt-smoke <model.gguf>` loads the engine through
    // the shipped install layout and transcribes a beat of silence.
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        let argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
        if let Some(i) = argv.iter().position(|a| a == "--replyvoice-stt-smoke") {
            let Some(model) = argv.get(i + 1) else {
                eprintln!("usage: --replyvoice-stt-smoke <model.gguf>");
                std::process::exit(2);
            };
            match reply_voice::stt::smoke(std::path::Path::new(model)) {
                Ok(detail) => {
                    println!("STT SMOKE OK: {detail}");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("STT SMOKE FAILED: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
    ai_chat_pipeline_lib::run()
}
