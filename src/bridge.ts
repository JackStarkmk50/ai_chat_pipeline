// Frontend <-> Rust bridge. In a plain browser (no Tauri), a small in-memory
// mock backs the same contract so screens can be exercised and screenshotted.

export type BridgeName = "not_connected" | "waiting" | "received";
export type VoiceKind = "piper" | "system" | "none";

export type DownloadState =
  | { state: "idle" }
  | { state: "downloading"; done: number; total: number }
  | { state: "failed"; message: string }
  | { state: "done" };

export type ListenPhase = "idle" | "recording" | "transcribing";

export interface SttModelInfo {
  id: string;
  label: string;
  description: string;
  mb: number;
  installed: boolean;
  license: string;
}

export interface UiState {
  bridge: BridgeName;
  link_error: string | null;
  speaking: boolean;
  voice_kind: VoiceKind;
  voice_name: string;
  voice_ready: boolean;
  has_manifest: boolean;
  manifest_mb: number | null;
  download: DownloadState;
  can_replay: boolean;
  first_run_done: boolean;
  last_error: string | null;
  listen_supported: boolean;
  listen_phase: ListenPhase;
  listen_pending: number;
  transcript: string[];
  listen_error: string | null;
  stt_model_label: string;
  stt_model_ready: boolean;
  stt_has_manifest: boolean;
  stt_download: DownloadState;
  stt_models: SttModelInfo[];
}

export interface Draft {
  voice: string;
  speed: number;
  start_with_computer: boolean;
  address: string;
  token: string;
  stt_model: string;
  first_run_done: boolean;
}

export interface Bridge {
  getState(): Promise<UiState>;
  getDraft(): Promise<Draft>;
  validate(draft: Draft): Promise<string | null>;
  save(draft: Draft): Promise<string | null>;
  makeToken(): Promise<string>;
  playSample(): Promise<void>;
  downloadVoice(): Promise<string | null>;
  cancelDownload(): Promise<void>;
  replay(): Promise<string | null>;
  hideWindow(): Promise<void>;
  listenStart(): Promise<string | null>;
  listenStop(): Promise<void>;
  downloadStt(id: string): Promise<string | null>;
  cancelSttDownload(): Promise<void>;
  openLink(url: string): Promise<void>;
  onState(cb: (s: UiState) => void): void;
}

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

const DEFAULT_DRAFT: Draft = {
  voice: "English, default voice",
  speed: 1.0,
  start_with_computer: false,
  address: "127.0.0.1:47615",
  token: "",
  stt_model: "",
  first_run_done: false,
};

function tauriBridge(): Bridge | null {
  // Tauri v2 injects __TAURI_INTERNALS__ into the webview; a plain browser has none.
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (!w.__TAURI_INTERNALS__) return null;
  return {
    getState: () => invoke<UiState>("get_state"),
    getDraft: () => invoke<Draft>("get_draft"),
    validate: (draft) => invoke<string | null>("validate_settings", { draft }),
    save: (draft) => invoke<string | null>("save_settings", { draft }),
    makeToken: () => invoke<string>("make_token"),
    playSample: () => invoke<void>("play_sample"),
    downloadVoice: () => invoke<string | null>("download_voice"),
    cancelDownload: () => invoke<void>("cancel_download"),
    replay: () => invoke<string | null>("replay_last"),
    hideWindow: () => invoke<void>("hide_window"),
    listenStart: () => invoke<string | null>("listen_start"),
    listenStop: () => invoke<void>("listen_stop"),
    downloadStt: (id) => invoke<string | null>("download_stt_model", { id }),
    cancelSttDownload: () => invoke<void>("cancel_stt_download"),
    openLink: (url) => openUrl(url),
    onState(cb) {
      listen<UiState>("rv-state", (e) => cb(e.payload));
    },
  };
}

// ---- Browser mock (screenshots and manual UI checks only) ----

function mockBridge(): Bridge {
  const hash = window.location.hash.replace(/^#/, "") || "home-waiting";
  const state: UiState = {
    bridge: "waiting",
    link_error: null,
    speaking: false,
    voice_kind: "system",
    voice_name: "",
    voice_ready: true,
    has_manifest: false,
    manifest_mb: null,
    download: { state: "idle" },
    can_replay: false,
    first_run_done: true,
    last_error: null,
    listen_supported: true,
    listen_phase: "idle",
    listen_pending: 0,
    transcript: [],
    listen_error: null,
    stt_model_label: "",
    stt_model_ready: false,
    stt_has_manifest: true,
    stt_download: { state: "idle" },
    stt_models: [
      {
        id: "parakeet-unified-en-0.6b",
        label: "Parakeet English 0.6B",
        description: "Handy's recommended all-around pick for English dictation.",
        mb: 731,
        installed: false,
        license: "NVIDIA Open Model License",
      },
      {
        id: "whisper-large-v3-turbo",
        label: "Whisper Large v3 Turbo",
        description: "More accurate; best with a GPU.",
        mb: 886,
        installed: false,
        license: "mit",
      },
      {
        id: "whisper-small",
        label: "Whisper Small",
        description: "Smallest download; lower accuracy.",
        mb: 270,
        installed: false,
        license: "mit",
      },
    ],
  };
  const draft: Draft = { ...DEFAULT_DRAFT };
  let listeners: ((s: UiState) => void)[] = [];
  const emit = () => listeners.forEach((cb) => cb({ ...state }));

  switch (hash) {
    case "home-not-connected":
      state.bridge = "not_connected";
      break;
    case "home-recording":
      state.stt_model_ready = true;
      state.stt_model_label = "Parakeet English 0.6B";
      state.listen_phase = "recording";
      break;
    case "home-transcript":
      state.stt_model_ready = true;
      state.stt_model_label = "Parakeet English 0.6B";
      state.transcript = [
        "Just replied to Sarah's message about the project meeting.",
        "Also sent her the updated timeline.",
      ];
      break;
    case "home-model-ready":
      state.stt_model_ready = true;
      state.stt_model_label = "Parakeet English 0.6B";
      break;
    case "home-transcribing":
      state.stt_model_ready = true;
      state.stt_model_label = "Parakeet English 0.6B";
      state.listen_phase = "transcribing";
      break;
    case "settings-stt-downloading":
      state.stt_download = { state: "downloading", done: 280, total: 731 };
      break;
    case "home-received":
      state.bridge = "received";
      state.can_replay = true;
      break;
    case "home-speaking":
      state.bridge = "received";
      state.speaking = true;
      state.can_replay = true;
      break;
    case "home-error":
      state.bridge = "not_connected";
      state.last_error = "The voice stopped with an error";
      break;
    case "first-run-1":
      state.first_run_done = false;
      state.voice_kind = "none";
      state.voice_ready = false;
      state.bridge = "not_connected";
      break;
    case "first-run-1m":
      state.first_run_done = false;
      state.voice_kind = "none";
      state.voice_ready = false;
      state.bridge = "not_connected";
      state.has_manifest = true;
      state.manifest_mb = 90;
      break;
    case "first-run-1b":
      state.first_run_done = false;
      state.voice_kind = "none";
      state.voice_ready = false;
      state.bridge = "not_connected";
      state.has_manifest = true;
      state.manifest_mb = 90;
      state.download = { state: "downloading", done: 34, total: 90 };
      break;
    case "first-run-1c":
      state.first_run_done = false;
      state.voice_kind = "piper";
      state.voice_name = "Reply Voice voice";
      state.bridge = "not_connected";
      state.has_manifest = true;
      state.manifest_mb = 90;
      break;
    case "settings-manifest":
      state.has_manifest = true;
      state.manifest_mb = 90;
      break;
    case "first-run-2":
      state.first_run_done = false;
      state.bridge = "not_connected";
      draft.token = "";
      break;
    case "first-run-3":
      state.first_run_done = false;
      state.voice_kind = "none";
      state.voice_ready = false;
      state.bridge = "not_connected";
      break;
    case "first-run-3b":
      state.first_run_done = false;
      state.bridge = "received";
      state.can_replay = true;
      draft.token = "abcdef0123456789";
      break;
    case "settings-not-ready":
      state.voice_kind = "none";
      state.voice_ready = false;
      state.bridge = "not_connected";
      break;
    default:
      break;
  }

  return {
    getState: async () => ({ ...state }),
    getDraft: async () => ({ ...draft }),
    validate: async (d) => {
      if (!/^(127\.\d{1,3}\.\d{1,3}\.\d{1,3}|localhost|\[?::1\]?):\d+$/.test(d.address.trim()))
        return "Only this computer (127.0.0.1) is allowed";
      if (d.token && d.token.length < 16) return "The token needs at least 16 characters";
      return null;
    },
    save: async (d) => {
      Object.assign(draft, d);
      state.first_run_done = d.first_run_done;
      state.bridge = d.token ? "waiting" : "not_connected";
      emit();
      return null;
    },
    makeToken: async () => "9f27c1ab04e34d2b8a61f0c37d5e88aa",
    playSample: async () => {},
    downloadVoice: async () => {
      state.download = { state: "downloading", done: 10, total: 90 };
      emit();
      return null;
    },
    cancelDownload: async () => {
      state.download = { state: "idle" };
      emit();
    },
    replay: async () => null,
    hideWindow: async () => {},
    listenStart: async () => {
      if (!state.stt_model_ready) return "No speech model on this computer yet. Download one in Settings.";
      state.listen_phase = "recording";
      emit();
      return null;
    },
    listenStop: async () => {
      state.listen_phase = "idle";
      emit();
    },
    downloadStt: async (id) => {
      state.stt_download = { state: "downloading", done: 100, total: 731 };
      const m = state.stt_models.find((x) => x.id === id);
      if (m) m.installed = true;
      state.stt_model_ready = true;
      state.stt_model_label = m?.label ?? "";
      draft.stt_model = id;
      emit();
      return null;
    },
    cancelSttDownload: async () => {
      state.stt_download = { state: "idle" };
      emit();
    },
    openLink: async (url) => {
      window.open(url, "_blank", "noopener");
    },
    onState(cb) {
      listeners.push(cb);
    },
  };
}

export const bridge: Bridge = tauriBridge() ?? mockBridge();

// Mock-only: which screen a #hash screenshot should open on.
export function mockInitialScreen():
  | { kind: "first"; step: number }
  | { kind: "settings" }
  | null {
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  if (w.__TAURI_INTERNALS__) return null;
  const hash = window.location.hash.replace(/^#/, "");
  const m = hash.match(/^first-run-(\d)/);
  if (m) return { kind: "first", step: Number(m[1]) };
  if (hash.startsWith("settings")) return { kind: "settings" };
  return null;
}
