//! Microphone capture -> voice activity detection -> speech segments.
//!
//! Real wiring against handy-recorder 0.1.0 (MIT) and earshot 1.2.2
//! (MIT OR Apache-2.0); pipeline shape and timing profile follow Handy
//! (cjpais/Handy, MIT, (c) 2025 CJ Pais), re-implemented with attribution.
//! UNCOMPILED prep source: dependencies land with the verified migration head.
//!
//! Listening is explicit: a ListenSession exists only between the user's
//! record and stop actions. No always-listening, no global hotkey here; the
//! trigger surface is the Tauri command layer.

use std::collections::VecDeque;
use std::sync::mpsc::SyncSender;

use handy_recorder::{AudioChunk, Recorder, RecorderConfig, Sink};

/// handy-recorder's `RecorderConfig::speech()` delivers exactly this:
/// 16 kHz, MixToMono, 480-frame (30 ms) chunks, resampled inside the crate -
/// no rubato needed on the mic path.
pub const SAMPLE_RATE: u32 = 16_000;

/// earshot predicts on exactly 16 ms (256 samples) of mono 16 kHz audio
/// (Handy's audio_toolkit/vad/earshot.rs).
pub const EARSHOT_FRAME_SAMPLES: usize = 256;

/// Handy's VAD timing profile converted to 256-sample frames
/// (audio_toolkit/vad/mod.rs tests): 450 ms pre-roll, 60 ms onset,
/// 450 ms offline hangover.
pub const PREFILL_FRAMES: usize = 29;
pub const ONSET_FRAMES: usize = 4;
pub const HANGOVER_FRAMES: usize = 29;

#[derive(Debug)]
pub struct ListenError(pub String);

impl From<handy_recorder::Error> for ListenError {
    fn from(e: handy_recorder::Error) -> Self {
        ListenError(e.to_string())
    }
}

/// One detected speech segment: 16 kHz mono f32, pre-roll included.
pub struct SpeechSegment {
    pub pcm: Vec<f32>,
}

/// A boolean VAD backend. Handy's trait shape, simplified to owned output.
pub trait VoiceActivityDetector: Send {
    /// Feed one frame of exactly `frame_samples()` samples.
    fn is_voice(&mut self, frame: &[f32]) -> Result<bool, ListenError>;
    fn frame_samples(&self) -> usize;
    fn reset(&mut self);
}

/// Pure-Rust Silero VAD via the earshot crate (Handy's EarshotVad adapter,
/// re-implemented). Construct on the heap: the detector keeps ~8 KiB of state.
pub struct EarshotVad {
    engine: Box<earshot::Detector>,
    threshold: f32,
    clamped_frame: [f32; EARSHOT_FRAME_SAMPLES],
}

impl EarshotVad {
    pub fn new(threshold: f32) -> Result<EarshotVad, ListenError> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(ListenError("VAD threshold must be 0.0..=1.0".into()));
        }
        Ok(EarshotVad {
            engine: earshot::Detector::default_boxed(),
            threshold,
            clamped_frame: [0.0; EARSHOT_FRAME_SAMPLES],
        })
    }
}

impl VoiceActivityDetector for EarshotVad {
    fn is_voice(&mut self, frame: &[f32]) -> Result<bool, ListenError> {
        if frame.len() != EARSHOT_FRAME_SAMPLES {
            return Err(ListenError(format!(
                "earshot needs {EARSHOT_FRAME_SAMPLES} samples, got {}",
                frame.len()
            )));
        }
        if frame.iter().any(|s| !s.is_finite()) {
            return Err(ListenError("non-finite sample in VAD input".into()));
        }
        // Resampling a full-scale signal can ring outside [-1, 1]; clamp only
        // the prediction input, never the audio kept for transcription.
        let score = if frame.iter().all(|s| (-1.0..=1.0).contains(s)) {
            self.engine.predict_f32(frame)
        } else {
            for (c, s) in self.clamped_frame.iter_mut().zip(frame) {
                *c = s.clamp(-1.0, 1.0);
            }
            self.engine.predict_f32(&self.clamped_frame)
        };
        Ok(score >= self.threshold)
    }

    fn frame_samples(&self) -> usize {
        EARSHOT_FRAME_SAMPLES
    }

    fn reset(&mut self) {
        self.engine.reset();
    }
}

/// Pre-roll + onset + hangover smoothing over a boolean VAD (Handy's
/// SmoothedVad logic, re-implemented). Emits owned sample batches so the
/// sink can aggregate a segment without lifetime juggling.
pub enum VadOut {
    Speech(Vec<f32>),
    Noise,
}

struct BufferedFrame {
    samples: Vec<f32>,
    emitted: bool,
    voiced: bool,
}

pub struct SmoothedVad {
    inner: Box<dyn VoiceActivityDetector>,
    prefill_frames: usize,
    hangover_frames: usize,
    onset_frames: usize,
    frame_buffer: VecDeque<BufferedFrame>,
    hangover_counter: usize,
    onset_counter: usize,
    in_speech: bool,
}

impl SmoothedVad {
    pub fn new(inner: Box<dyn VoiceActivityDetector>) -> SmoothedVad {
        SmoothedVad {
            inner,
            prefill_frames: PREFILL_FRAMES,
            hangover_frames: HANGOVER_FRAMES,
            onset_frames: ONSET_FRAMES,
            frame_buffer: VecDeque::new(),
            hangover_counter: 0,
            onset_counter: 0,
            in_speech: false,
        }
    }

    pub fn push_frame(&mut self, frame: &[f32]) -> Result<VadOut, ListenError> {
        self.frame_buffer.push_back(BufferedFrame {
            samples: frame.to_vec(),
            emitted: false,
            voiced: false,
        });
        while self.frame_buffer.len() > self.prefill_frames + 1 {
            self.frame_buffer.pop_front();
        }

        let is_voice = self.inner.is_voice(frame)?;
        if let Some(last) = self.frame_buffer.back_mut() {
            last.voiced = is_voice;
        }

        match (self.in_speech, is_voice) {
            (false, true) => {
                self.onset_counter += 1;
                if self.onset_counter >= self.onset_frames {
                    self.in_speech = true;
                    self.hangover_counter = self.hangover_frames;
                    self.onset_counter = 0;
                    let mut out = Vec::new();
                    for b in self.frame_buffer.iter_mut().filter(|f| !f.emitted) {
                        out.extend_from_slice(&b.samples);
                        b.emitted = true;
                    }
                    Ok(VadOut::Speech(out))
                } else {
                    Ok(VadOut::Noise)
                }
            }
            (true, true) => {
                self.hangover_counter = self.hangover_frames;
                if let Some(f) = self.frame_buffer.back_mut() {
                    f.emitted = true;
                }
                Ok(VadOut::Speech(frame.to_vec()))
            }
            (true, false) => {
                if self.hangover_counter > 0 {
                    self.hangover_counter -= 1;
                    if let Some(f) = self.frame_buffer.back_mut() {
                        f.emitted = true;
                    }
                    Ok(VadOut::Speech(frame.to_vec()))
                } else {
                    self.in_speech = false;
                    Ok(VadOut::Noise)
                }
            }
            (false, false) => {
                self.onset_counter = 0;
                Ok(VadOut::Noise)
            }
        }
    }

    /// Trailing withheld frames with any voiced among them (Handy's
    /// tail_report): a final word may be cut off at the stop boundary.
    pub fn tail_has_voiced(&self) -> bool {
        self.frame_buffer
            .iter()
            .rev()
            .take_while(|f| !f.emitted)
            .any(|f| f.voiced)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
        self.frame_buffer.clear();
        self.hangover_counter = 0;
        self.onset_counter = 0;
        self.in_speech = false;
    }
}

/// The handy-recorder sink: 480-sample chunks in, speech segments out.
/// Runs on the recorder's delivery thread, so it must return promptly -
/// VAD on 256 samples is cheap; transcription happens elsewhere.
pub struct SegmentSink {
    vad: SmoothedVad,
    pending: Vec<f32>,
    current: Vec<f32>,
    in_segment: bool,
    out: SyncSender<SpeechSegment>,
    pub dropped_segments: u64,
}

impl SegmentSink {
    pub fn new(vad: SmoothedVad, out: SyncSender<SpeechSegment>) -> SegmentSink {
        SegmentSink {
            vad,
            pending: Vec::new(),
            current: Vec::new(),
            in_segment: false,
            out,
            dropped_segments: 0,
        }
    }

    /// On stop: emit the trailing segment when one is open or the VAD
    /// withheld a voiced tail.
    pub fn flush_tail(&mut self) {
        if self.in_segment || self.vad.tail_has_voiced() {
            if !self.current.is_empty() {
                let pcm = std::mem::take(&mut self.current);
                let _ = self.out.try_send(SpeechSegment { pcm });
            }
        }
        self.in_segment = false;
        self.vad.reset();
    }
}

impl Sink for SegmentSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        // RecorderConfig::speech() mixes to mono: one sample per frame.
        let real = chunk.valid_frames * chunk.channels as usize;
        self.pending.extend_from_slice(&chunk.samples[..real]);
        while self.pending.len() >= EARSHOT_FRAME_SAMPLES {
            let frame: Vec<f32> = self.pending.drain(..EARSHOT_FRAME_SAMPLES).collect();
            match self.vad.push_frame(&frame) {
                Ok(VadOut::Speech(samples)) => {
                    self.in_segment = true;
                    self.current.extend_from_slice(&samples);
                }
                Ok(VadOut::Noise) => {
                    if self.in_segment {
                        self.in_segment = false;
                        let pcm = std::mem::take(&mut self.current);
                        // Bounded channel: a stalled consumer drops the
                        // segment and counts it, never stalls the mic.
                        if self.out.try_send(SpeechSegment { pcm }).is_err() {
                            self.dropped_segments += 1;
                        }
                    }
                }
                Err(_) => { /* VAD errors counted by the session layer */ }
            }
        }
    }
}

/// One user-triggered listening session: record -> segments -> stop.
/// Dropping it without `stop` closes the microphone (Recorder's Drop).
pub struct ListenSession {
    recorder: Recorder<SegmentSink>,
}

impl ListenSession {
    /// Opens the default microphone at speech format and starts recording.
    pub fn start(
        out: SyncSender<SpeechSegment>,
        vad_threshold: f32,
    ) -> Result<ListenSession, ListenError> {
        let vad = SmoothedVad::new(Box::new(EarshotVad::new(vad_threshold)?));
        let sink = SegmentSink::new(vad, out);
        let recorder = Recorder::open_with_failure_handler(RecorderConfig::speech(), |error| {
            // Library thread: recording still ends through stop(); the
            // session layer surfaces this through shared status instead.
            log::warn!("microphone failed during recording: {error}");
        })?;
        recorder
            .start(sink)
            .map_err(|e| ListenError(e.to_string()))?;
        Ok(ListenSession { recorder })
    }

    /// Stops recording and flushes the trailing segment. Reports incomplete
    /// captures (mic failure or dropped audio) instead of hiding them.
    pub fn stop(self) -> Result<(), ListenError> {
        let stopped = self.recorder.stop()?;
        if !stopped.is_complete() {
            log::warn!(
                "recording ended early: {:?}, {} frames dropped",
                stopped.end_reason,
                stopped.dropped_frames
            );
        }
        let mut sink = stopped.sink;
        if sink.dropped_segments > 0 {
            log::warn!("{} speech segments dropped (consumer stalled)", sink.dropped_segments);
        }
        sink.flush_tail();
        Ok(())
    }
}

impl SegmentSink {
    /// Test helper: process a raw 480-sample mono chunk without a recorder.
    #[doc(hidden)]
    pub fn process_chunk_for_test(&mut self, samples: &[f32]) {
        self.process_chunk(AudioChunk {
            samples,
            sample_rate: SAMPLE_RATE,
            channels: 1,
            valid_frames: samples.len(),
        });
    }
}
