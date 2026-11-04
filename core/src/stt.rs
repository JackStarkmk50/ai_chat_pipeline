//! Speech-to-text engine supervision. Engine- and model-agnostic at the
//! trait boundary; the concrete engine is transcribe-cpp 0.4 (MIT,
//! handy-computer/transcribe.cpp) running GGUF models - Handy's engine.
//!
//! Process isolation and the wire protocol are adopted from Handy's
//! engine_supervisor (cjpais/Handy, MIT, (c) 2025 CJ Pais): all native engine
//! code runs in a child worker process (the app re-launched with a worker
//! flag), so a GPU driver fault kills only the worker; on a GPU fault the
//! supervisor retries once in a CPU-only worker. One model loaded at a time;
//! killing the worker returns its CPU/GPU memory to the OS.
//!

use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub mod protocol {
    //! Wire protocol between Cue and its STT worker process.
    //! Frame format adopted from Handy's engine_supervisor/protocol.rs
    //! (MIT, (c) 2025 CJ Pais): `[u32 json_len][json][u32 pcm_len][pcm LE f32]`.
    //! JSON is deliberate: transcribe-cpp's serde impls round-trip NaN
    //! confidences through `null`, which only works self-describing.
    //!
    //! Streaming messages (StreamBegin/Feed/Finalize/StreamReset in Handy)
    //! are left out of v1: both current candidates run buffered. Adding them
    //! later is wire-compatible - same frame format, new enum variants.

    use serde::{Deserialize, Serialize};
    use std::io::{self, Read, Write};
    use std::path::PathBuf;
    use transcribe_cpp::{Backend, Capabilities, DeviceType, RunOptions, Transcript};

    /// Refuse frames larger than this (one hour of 16 kHz f32 PCM is ~230 MB).
    const MAX_SECTION_BYTES: usize = 1 << 30;

    #[derive(Debug, Serialize, Deserialize)]
    pub enum Request {
        /// First request to every worker; answered once backend init is done.
        Hello { list_devices: bool },
        /// Unloads any previous model first, then loads.
        Load { path: PathBuf, backend: Backend, device: DeviceSelector },
        /// Followed by the PCM to transcribe.
        Run { options: RunOptions },
    }

    /// How the worker picks the compute device (device handles are
    /// process-local, so the parent names devices by stable key or index).
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum DeviceSelector {
        Auto,
        Key(String),
        Index(usize),
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub enum Response {
        Hello { devices: Option<Vec<DeviceInfo>> },
        Loaded(LoadedInfo),
        Transcript(Transcript),
        Ok,
        Error(String),
    }

    /// Serializable mirror of transcribe_cpp::Device (process-local handle).
    /// Shape and key format adopted from Handy's protocol.rs (MIT) so a
    /// persisted device choice stays stable across processes.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DeviceInfo {
        pub name: String,
        pub description: String,
        pub kind: String,
        pub device_type: DeviceType,
        pub device_id: Option<String>,
        pub memory_total: u64,
        pub index: Option<usize>,
        /// Stable identity for persisting a device choice; see `device_key`.
        pub key: String,
    }

    impl DeviceInfo {
        pub fn from_device(device: &transcribe_cpp::Device) -> Self {
            DeviceInfo {
                name: device.name.clone(),
                description: device.description.clone(),
                kind: device.kind.clone(),
                device_type: device.device_type,
                device_id: device.device_id.clone(),
                memory_total: device.memory_total,
                index: device.index,
                key: device_key(&device.kind, device.device_id.as_deref(), &device.name),
            }
        }

        pub fn label(&self) -> &str {
            if self.description.is_empty() { &self.name } else { &self.description }
        }

        pub fn is_gpu(&self) -> bool {
            matches!(self.device_type, DeviceType::Gpu | DeviceType::Igpu)
        }
    }

    /// Persistent device identity (Handy's exact format, adopted MIT): the
    /// backend's stable device_id where available, else its name.
    pub fn device_key(kind: &str, device_id: Option<&str>, name: &str) -> String {
        let (identity_kind, identity) = match device_id {
            Some(id) => ("id", id),
            None => ("name", name),
        };
        serde_json::to_string(&(kind, identity_kind, identity))
            .expect("device identity is always JSON serializable")
    }

    /// What the parent needs to know about a loaded model, read once at load.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LoadedInfo {
        pub arch: String,
        pub variant: String,
        pub backend: String,
        pub device: String,
        /// Whether the bound device is a GPU (a crash there may be its fault).
        pub on_gpu: bool,
        pub capabilities: Capabilities,
        /// Whether the model accepts audio of any length in one run
        /// (capabilities.max_audio_ms == 0); other models get their long
        /// audio chunked by the caller.
        pub supports_long_form: bool,
    }

    pub fn encode_message<T: Serialize>(message: &T, pcm: Option<&[f32]>) -> io::Result<Vec<u8>> {
        let json = serde_json::to_vec(message).map_err(io::Error::other)?;
        if json.len() > MAX_SECTION_BYTES {
            return Err(io::Error::other("frame section too large"));
        }
        let pcm = pcm.unwrap_or(&[]);
        if pcm.len() * 4 > MAX_SECTION_BYTES {
            return Err(io::Error::other("audio too long for one transcription"));
        }
        let mut frame = Vec::with_capacity(8 + json.len() + pcm.len() * 4);
        frame.extend_from_slice(&(json.len() as u32).to_le_bytes());
        frame.extend_from_slice(&json);
        frame.extend_from_slice(&((pcm.len() * 4) as u32).to_le_bytes());
        for sample in pcm {
            frame.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(frame)
    }

    pub fn write_message<T: Serialize>(
        w: &mut impl Write,
        message: &T,
        pcm: Option<&[f32]>,
    ) -> io::Result<()> {
        w.write_all(&encode_message(message, pcm)?)?;
        w.flush()
    }

    /// `Ok(None)` only on a clean EOF before a frame; a cut frame is an error.
    pub fn read_message<T: for<'de> Deserialize<'de>>(
        r: &mut impl Read,
    ) -> io::Result<Option<(T, Vec<f32>)>> {
        let mut len = [0u8; 4];
        let mut filled = 0;
        while filled < len.len() {
            match r.read(&mut len[filled..]) {
                Ok(0) if filled == 0 => return Ok(None),
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        let json = read_section(r, len)?;
        let message = serde_json::from_slice(&json).map_err(io::Error::other)?;
        let pcm_bytes = read_section(r, [0; 4])?;
        if pcm_bytes.len() % 4 != 0 {
            return Err(io::Error::other("PCM section is not whole f32s"));
        }
        let pcm = pcm_bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        Ok(Some((message, pcm)))
    }

    fn read_section(r: &mut impl Read, mut len: [u8; 4]) -> io::Result<Vec<u8>> {
        // `len` carries a prefix already read when non-zero; zero means read it.
        if len == [0; 4] {
            r.read_exact(&mut len)?;
        }
        let n = u32::from_le_bytes(len) as usize;
        if n > MAX_SECTION_BYTES {
            return Err(io::Error::other("frame section too large"));
        }
        let mut buf = vec![0u8; n];
        r.read_exact(&mut buf)?;
        Ok(buf)
    }
}

use protocol::{DeviceSelector, LoadedInfo, Request, Response};

/// Hidden flags: the app re-launches itself as a worker (Handy's pattern).
pub const WORKER_FLAG: &str = "--replyvoice-stt-worker";
pub const CPU_ONLY_FLAG: &str = "--replyvoice-stt-cpu-only";

#[derive(Debug)]
pub enum SttError {
    ModelMissing(PathBuf),
    WorkerCrashed(String),
    Engine(String),
    Io(std::io::Error),
}

impl From<std::io::Error> for SttError {
    fn from(e: std::io::Error) -> Self {
        SttError::Io(e)
    }
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SttError::ModelMissing(p) => write!(f, "The speech model file is missing: {}", p.display()),
            SttError::WorkerCrashed(m) => write!(f, "The transcription worker stopped: {m}"),
            SttError::Engine(m) => write!(f, "The speech engine reported: {m}"),
            SttError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: std::io::BufReader<ChildStdout>,
}

/// Owns the worker process. The rest of the app never sees processes,
/// restarts, or backend fallback - Handy's supervisor contract.
pub struct Supervisor {
    worker: Option<Worker>,
    cpu_only: bool,
    model_path: Option<PathBuf>,
    loaded: Option<LoadedInfo>,
}

impl Supervisor {
    pub fn new() -> Supervisor {
        Supervisor { worker: None, cpu_only: false, model_path: None, loaded: None }
    }

    fn spawn(&mut self) -> Result<(), SttError> {
        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.arg(WORKER_FLAG);
        if self.cpu_only {
            cmd.arg(CPU_ONLY_FLAG);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or(SttError::WorkerCrashed("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .map(std::io::BufReader::new)
            .ok_or(SttError::WorkerCrashed("no stdout".into()))?;
        let mut worker = Worker { child, stdin, stdout };
        // Handshake: backend init happens before Hello is answered, so this
        // is where a broken GPU driver surfaces.
        protocol::write_message(&mut worker.stdin, &Request::Hello { list_devices: false }, None)?;
        match protocol::read_message::<Response>(&mut worker.stdout)? {
            Some((Response::Hello { .. }, _)) => {}
            Some((Response::Error(e), _)) => return Err(SttError::Engine(e)),
            _ => return Err(SttError::WorkerCrashed("unexpected hello reply".into())),
        }
        self.worker = Some(worker);
        Ok(())
    }

    /// Load a model, spawning the worker if needed. One model at a time:
    /// the worker frees the old model before loading the new one.
    pub fn load(&mut self, model_path: &Path) -> Result<(), SttError> {
        if !model_path.is_file() {
            return Err(SttError::ModelMissing(model_path.to_path_buf()));
        }
        if self.worker.is_none() {
            self.spawn()?;
        }
        let reply = self.roundtrip(&Request::Load {
            path: model_path.to_path_buf(),
            backend: transcribe_cpp::Backend::Auto,
            device: DeviceSelector::Auto,
        }, None)?;
        match reply {
            Response::Loaded(info) => {
                self.model_path = Some(model_path.to_path_buf());
                self.loaded = Some(info);
                Ok(())
            }
            Response::Error(e) => Err(SttError::Engine(e)),
            _ => Err(SttError::WorkerCrashed("unexpected load reply".into())),
        }
    }

    /// Blocking transcription of one speech segment (16 kHz mono f32).
    /// On a GPU-worker crash, restarts CPU-only and retries once (Handy's
    /// recovery path); the caller never sees the restart.
    pub fn transcribe(&mut self, pcm: &[f32]) -> Result<transcribe_cpp::Transcript, SttError> {
        match self.run_once(pcm) {
            Ok(t) => Ok(t),
            Err(e) => {
                let gpu_at_fault = self.loaded.as_ref().map(|l| l.on_gpu).unwrap_or(false);
                if gpu_at_fault && !self.cpu_only {
                    self.kill();
                    self.cpu_only = true;
                    let model = self
                        .model_path
                        .clone()
                        .ok_or(SttError::WorkerCrashed("no model to reload".into()))?;
                    self.load(&model)?;
                    self.run_once(pcm)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn run_once(&mut self, pcm: &[f32]) -> Result<transcribe_cpp::Transcript, SttError> {
        if self.worker.is_none() {
            return Err(SttError::WorkerCrashed("no worker".into()));
        }
        let reply = self.roundtrip(
            &Request::Run { options: transcribe_cpp::RunOptions::default() },
            Some(pcm),
        )?;
        match reply {
            Response::Transcript(t) => Ok(t),
            Response::Error(e) => Err(SttError::Engine(e)),
            _ => Err(SttError::WorkerCrashed("unexpected run reply".into())),
        }
    }

    fn roundtrip(&mut self, req: &Request, pcm: Option<&[f32]>) -> Result<Response, SttError> {
        let worker = self.worker.as_mut().ok_or(SttError::WorkerCrashed("no worker".into()))?;
        protocol::write_message(&mut worker.stdin, req, pcm)?;
        match protocol::read_message::<Response>(&mut worker.stdout)? {
            Some((resp, _)) => Ok(resp),
            None => Err(SttError::WorkerCrashed("worker closed the protocol".into())),
        }
    }

    /// Killing the worker returns all of its CPU/GPU memory to the OS.
    pub fn unload(&mut self) {
        self.kill();
    }

    fn kill(&mut self) {
        if let Some(mut w) = self.worker.take() {
            let _ = w.child.kill();
            let _ = w.child.wait();
        }
        self.loaded = None;
    }
}

/// The worker process entry point: the only place transcribe.cpp native code
/// runs. Called from main() when WORKER_FLAG is present, before Tauri starts.
/// Mirrors Handy's worker.rs essentials.
/// Protocol stdout claim (unix): duplicate fd 1 for the protocol, then point
/// fd 1 at stderr so ggml's direct stdout writes can't corrupt the stream.
/// Mirrors Handy's take_stdout_for_protocol. Windows needs the _dup/_dup2
/// equivalent - TODO(verified-head), see Handy's worker.rs.
#[cfg(unix)]
fn claim_stdout_for_protocol() -> std::io::Result<std::fs::File> {
    use std::os::unix::io::FromRawFd;
    unsafe {
        let protocol_fd = libc::dup(1);
        if protocol_fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::dup2(2, 1) < 0 {
            let e = std::io::Error::last_os_error();
            libc::close(protocol_fd);
            return Err(e);
        }
        Ok(std::fs::File::from_raw_fd(protocol_fd))
    }
}

/// Windows: native code writes through the CRT's fd 1, so redirect that one;
/// the protocol uses the OS handle behind a CRT duplicate of the original
/// (Handy's take_stdout_for_protocol, adopted MIT).
#[cfg(windows)]
fn claim_stdout_for_protocol() -> std::io::Result<std::fs::File> {
    use std::os::windows::io::FromRawHandle;
    unsafe {
        let fd = libc::dup(1);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::dup2(2, 1) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let handle = libc::get_osfhandle(fd);
        if handle == -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(std::fs::File::from_raw_handle(handle as _))
    }
}

#[cfg(not(any(unix, windows)))]
fn claim_stdout_for_protocol() -> std::io::Result<std::fs::File> {
    Err(std::io::Error::other("stdout claim not implemented on this OS"))
}

/// Headless engine proof for packaged builds: spawn the worker from THIS
/// executable, load the model through the staged dynamic backends, and
/// transcribe a beat of silence. Success means the shipped binary found and
/// loaded libtranscribe + the ggml modules from its install layout.
pub fn smoke(model_path: &std::path::Path) -> Result<String, SttError> {
    let mut sup = Supervisor::new();
    sup.load(model_path)?;
    let silence = vec![0.0f32; 16_000];
    let text = match sup.transcribe(&silence) {
        Ok(t) => t.text,
        Err(e) => {
            let _ = sup.unload();
            return Err(e);
        }
    };
    sup.unload();
    Ok(text)
}

pub fn run_worker() -> i32 {
    use std::io::BufReader;
    use transcribe_cpp::{BackendMask, Model};

    let cpu_only = std::env::args_os().any(|a| a == CPU_ONLY_FLAG);

    // Claim the protocol stream BEFORE any native code runs: ggml writes to
    // stdout in places, and one stray byte corrupts the protocol.
    let protocol_out = match claim_stdout_for_protocol() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("stt worker: failed to claim stdout: {e}");
            return 2;
        }
    };
    transcribe_cpp::init_logging();
    let init = if cpu_only {
        transcribe_cpp::init_backends_with(None::<&Path>, BackendMask::CPU)
    } else {
        transcribe_cpp::init_backends_default()
    };
    let init_error = init.err().map(|e| format!("backend init failed: {e}"));

    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = protocol_out;
    let mut session: Option<(transcribe_cpp::Session, LoadedInfo)> = None;

    loop {
        let incoming = match protocol::read_message::<Request>(&mut input) {
            Ok(Some(v)) => v,
            Ok(None) => return 0, // parent closed stdin: exit, freeing memory
            Err(e) => {
                log::error!("protocol read failed: {e}");
                return 1;
            }
        };
        let (request, pcm) = incoming;
        let response = match request {
            Request::Hello { list_devices } => match &init_error {
                Some(msg) => Response::Error(msg.clone()),
                None => Response::Hello {
                    devices: if list_devices {
                        Some(
                            transcribe_cpp::devices()
                                .iter()
                                .map(protocol::DeviceInfo::from_device)
                                .collect(),
                        )
                    } else {
                        None
                    },
                },
            },
            Request::Load { path, backend, device } => {
                session = None; // free any previous model first
                // Resolve the selector against this worker's devices; device
                // handles are process-local (Handy's load(), adopted MIT).
                let resolved = match &device {
                    protocol::DeviceSelector::Auto => None,
                    protocol::DeviceSelector::Key(key) => {
                        let found = transcribe_cpp::devices().into_iter().find(|d| {
                            let info = protocol::DeviceInfo::from_device(d);
                            info.is_gpu() && &info.key == key
                        });
                        if found.is_none() {
                            log::warn!(
                                "stored GPU device '{key}' no longer available; automatic selection"
                            );
                        }
                        found
                    }
                    protocol::DeviceSelector::Index(index) => {
                        match transcribe_cpp::devices().into_iter().find(|d| d.index == Some(*index)) {
                            Some(d) if matches!(
                                d.device_type,
                                transcribe_cpp::DeviceType::Accel | transcribe_cpp::DeviceType::Unknown
                            ) => {
                                let _ = protocol::write_message(
                                    &mut output,
                                    &Response::Error(format!(
                                        "device index {index} ({}) cannot host a model",
                                        d.kind
                                    )),
                                    None,
                                );
                                continue;
                            }
                            other => other,
                        }
                    }
                };
                match Model::load_with(&path, &transcribe_cpp::ModelOptions { backend, device: resolved }) {
                    Ok(m) => match (m.capabilities(), m.session()) {
                        (Ok(capabilities), Ok(sess)) => {
                            let bound = m.device().ok().map(|d| protocol::DeviceInfo::from_device(&d));
                            let backend_name = m.backend();
                            let info = LoadedInfo {
                                arch: m.arch(),
                                variant: m.variant(),
                                backend: backend_name.clone(),
                                device: bound
                                    .as_ref()
                                    .map(|d| d.label().to_string())
                                    .unwrap_or_else(|| "unknown".into()),
                                on_gpu: match &bound {
                                    Some(d) => d.is_gpu(),
                                    None => {
                                        !backend_name.is_empty()
                                            && !backend_name.eq_ignore_ascii_case("cpu")
                                    }
                                },
                                supports_long_form: m.supports(transcribe_cpp::Feature::LongForm),
                                capabilities,
                            };
                            session = Some((sess, info.clone()));
                            Response::Loaded(info)
                        }
                        (Err(e), _) => Response::Error(e.to_string()),
                        (_, Err(e)) => Response::Error(e.to_string()),
                    },
                    Err(e) => Response::Error(e.to_string()),
                }
            }
            Request::Run { options } => match session.as_mut() {
                Some((s, _)) => match s.run(&pcm, &options) {
                    Ok(t) => Response::Transcript(t),
                    Err(e) => Response::Error(e.to_string()),
                },
                None => Response::Error("no model loaded".into()),
            },
        };
        if protocol::write_message(&mut output, &response, None).is_err() {
            return 1;
        }
    }
    // Cancels never reach the worker: the parent kills it (Handy's design);
    // process exit reclaims model memory without running C++ teardown.
}
