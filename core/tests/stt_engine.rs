//! End-to-end engine validation: load the exact catalog artifact
//! (whisper-small Q8_0, pinned revision, SHA-256 verified) and transcribe a
//! real speech sample through transcribe-cpp's public API - the same calls
//! the Cue worker makes. Gated on env vars so CI without the model skips.
use std::path::PathBuf;

fn pcm(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect()
}

#[test]
fn loads_catalog_model_and_transcribes_speech() {
    let model = match std::env::var("RV_STT_MODEL") {
        Ok(m) => PathBuf::from(m),
        Err(_) => return, // model not present; skip
    };
    transcribe_cpp::init_backends_with(None::<&std::path::Path>, transcribe_cpp::BackendMask::CPU)
        .expect("backend init");
    let m = transcribe_cpp::Model::load(&model).expect("model load");
    let caps = m.capabilities().expect("capabilities");
    eprintln!("arch={} variant={} backend={}", m.arch(), m.variant(), m.backend());
    eprintln!("caps: streaming={} lang_detect={} max_audio_ms={} languages={}",
        caps.supports_streaming, caps.supports_language_detect, caps.max_audio_ms,
        caps.languages.len());

    let mut session = m.session().expect("session");

    // Real speech (13.7 s LibriSpeech sample): must produce non-empty text.
    let speech = pcm(&std::env::var("RV_STT_PCM").unwrap());
    let t = session.run(&speech, &transcribe_cpp::RunOptions::default()).expect("run speech");
    eprintln!("SPEECH TRANSCRIPT: {:?}", t.text);
    assert!(t.text.trim().len() > 10, "speech produced no transcript");

    // Silence: must not crash; text should be empty or near-empty.
    let silence = vec![0.0f32; 16_000 * 3];
    let t2 = session.run(&silence, &transcribe_cpp::RunOptions::default()).expect("run silence");
    eprintln!("SILENCE TRANSCRIPT: {:?}", t2.text);
    assert!(t2.text.trim().len() < 40, "silence hallucinated a long transcript");
}
