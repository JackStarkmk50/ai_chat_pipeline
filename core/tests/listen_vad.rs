//! VAD and sink tests with synthetic audio - no microphone involved.
use reply_voice::listen::*;
use handy_recorder::AudioChunk;
use std::sync::mpsc::sync_channel;

#[test]
fn earshot_rejects_wrong_frame_size() {
    let mut vad = EarshotVad::new(0.5).unwrap();
    assert!(vad.is_voice(&[0.0; EARSHOT_FRAME_SAMPLES - 1]).is_err());
}

#[test]
fn earshot_silence_is_not_voice() {
    let mut vad = EarshotVad::new(0.5).unwrap();
    assert!(!vad.is_voice(&[0.0; EARSHOT_FRAME_SAMPLES]).unwrap());
}

#[test]
fn earshot_rejects_bad_threshold_and_non_finite() {
    assert!(EarshotVad::new(-0.1).is_err());
    assert!(EarshotVad::new(1.1).is_err());
    let mut vad = EarshotVad::new(0.5).unwrap();
    let mut frame = [0.0; EARSHOT_FRAME_SAMPLES];
    frame[0] = f32::NAN;
    assert!(vad.is_voice(&frame).is_err());
}

/// Scripted boolean VAD for smoothing tests.
struct Scripted(std::collections::VecDeque<bool>);
impl VoiceActivityDetector for Scripted {
    fn is_voice(&mut self, _f: &[f32]) -> Result<bool, ListenError> {
        Ok(self.0.pop_front().unwrap_or(false))
    }
    fn frame_samples(&self) -> usize { EARSHOT_FRAME_SAMPLES }
    fn reset(&mut self) {}
}

fn frames(script: &[bool]) -> Vec<Vec<f32>> {
    script.iter().map(|_| vec![0.1; EARSHOT_FRAME_SAMPLES]).collect()
}

#[test]
fn smoothed_requires_onset_before_emitting() {
    // Fewer voiced frames than ONSET_FRAMES: nothing emitted.
    let mut vad = SmoothedVad::new(Box::new(Scripted(
        [true, true, false, false].into_iter().collect(),
    )));
    for f in frames(&[true, true, false, false]) {
        assert!(matches!(vad.push_frame(&f).unwrap(), VadOut::Noise));
    }
}

#[test]
fn smoothed_emits_preroll_on_confirmed_onset() {
    // ONSET_FRAMES consecutive voiced frames confirm speech; the emitted
    // batch includes buffered pre-roll (up to 29 frames + current).
    let script = [false, false, true, true, true, true, false];
    let mut vad = SmoothedVad::new(Box::new(Scripted(script.into_iter().collect())));
    let mut emitted_at = None;
    for (i, f) in frames(&script).into_iter().enumerate() {
        if let VadOut::Speech(samples) = vad.push_frame(&f).unwrap() {
            emitted_at = Some((i, samples.len()));
            break;
        }
    }
    let (i, len) = emitted_at.expect("onset never confirmed");
    assert_eq!(i, 5, "onset confirms on the 4th consecutive voiced frame");
    assert_eq!(len, 6 * EARSHOT_FRAME_SAMPLES, "2 pre-roll + 4 onset frames all flush at confirmation (Handy behaviour)");
}

#[test]
fn smoothed_hangover_then_segment_close_and_tail() {
    // Speech, then silence longer than the hangover: segment closes; the
    // withheld tail carries no voiced frames.
    let mut script = vec![true; 6];
    script.extend([false; 35]);
    let mut vad = SmoothedVad::new(Box::new(Scripted(script.clone().into_iter().collect())));
    let mut speech_frames = 0;
    let mut closed = false;
    for f in frames(&script) {
        match vad.push_frame(&f).unwrap() {
            VadOut::Speech(_) => speech_frames += 1,
            VadOut::Noise => {
                if speech_frames > 0 { closed = true; }
            }
        }
    }
    assert!(closed, "segment must close after the hangover");
    assert!(!vad.tail_has_voiced(), "trailing silence is not a voiced tail");
}

#[test]
fn sink_aggregates_chunks_into_one_segment() {
    // Feed 480-sample chunks: silence, then sustained "voice" ( Scripted vad
    // can't be used here - use a real SmoothedVad over an always-voiced stub
    // by threshold 0.0 on earshot would run the model; use the scripted path
    // through SegmentSink::new with a SmoothedVad over Scripted).
    let (tx, rx) = sync_channel(4);
    // Script: 4 voiced frames confirm onset and stay voiced while chunks last.
    let script = vec![true; 200];
    let vad = SmoothedVad::new(Box::new(Scripted(script.into_iter().collect())));
    let mut sink = SegmentSink::new(vad, tx);

    let chunk_audio = vec![0.05f32; 480];
    for _ in 0..20 {
        sink.process_chunk_for_test(&chunk_audio);
    }
    sink.flush_tail();
    let seg = rx.try_recv().expect("no segment emitted");
    assert!(seg.pcm.len() >= 20 * 480 - EARSHOT_FRAME_SAMPLES);
    assert!(rx.try_recv().is_err(), "exactly one segment expected");
}

#[test]
fn sink_drops_nothing_on_silence() {
    let (tx, rx) = sync_channel(4);
    let script = vec![false; 200];
    let vad = SmoothedVad::new(Box::new(Scripted(script.into_iter().collect())));
    let mut sink = SegmentSink::new(vad, tx);
    let chunk_audio = vec![0.0f32; 480];
    for _ in 0..10 {
        sink.process_chunk_for_test(&chunk_audio);
    }
    sink.flush_tail();
    assert!(rx.try_recv().is_err(), "silence must not produce a segment");
}
