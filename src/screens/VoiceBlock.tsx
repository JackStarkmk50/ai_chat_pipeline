import type { UiState } from "../bridge";
import { bridge } from "../bridge";
import { Card, OutlineButton, Pill, PrimaryButton, ProgressBar, SoftButton } from "../components";

function voiceLabel(s: UiState): string {
  if (s.voice_kind === "piper") return s.voice_name || "Reply Voice voice";
  if (s.voice_kind === "system") return "System voice (fallback)";
  return "English, default voice";
}

export function VoiceBlock({ state, inWizard }: { state: UiState; inWizard: boolean }) {
  const d = state.download;
  const sample = () => bridge.playSample();

  let body;
  if (d.state === "downloading") {
    body = (
      <Card>
        <ProgressBar done={d.done} total={d.total} />
      </Card>
    );
  } else if (d.state === "failed") {
    body = (
      <Card warn>
        <div className="text-[14px] font-medium text-rvwarn">The voice could not be downloaded</div>
        <div className="mt-0.5 text-[13px] text-rvtx2">{d.message}</div>
      </Card>
    );
  } else if (state.voice_kind === "piper") {
    body = (
      <Card>
        <div className="flex items-center gap-2.5">
          <Pill kind="ok">Ready</Pill>
          <span className="text-[14px] text-rvtx2">On this computer</span>
        </div>
      </Card>
    );
  } else {
    const system = state.voice_kind === "system";
    const title = system
      ? "Using the voice that came with this computer"
      : state.has_manifest
        ? "Voice not on this computer yet"
        : "No voice yet";
    const size =
      state.manifest_mb != null ? ` (about ${state.manifest_mb} MB, once)` : " (once)";
    const text = system
      ? state.has_manifest
        ? `Download the Reply Voice voice${size}.`
        : "No downloadable voice is set up. This voice is the fallback."
      : state.has_manifest
        ? `Download it${size}. Until then nothing can be read aloud.`
        : "No downloadable voice is set up and this computer has none. Nothing can be read aloud yet.";
    body = (
      <Card warn={!system}>
        <div className={`text-[14px] font-medium ${system ? "text-rvtx" : "text-rvwarn"}`}>{title}</div>
        <div className="mt-0.5 text-[13px] text-rvtx2">{text}</div>
      </Card>
    );
  }

  const downloading = d.state === "downloading";
  return (
    <div className="flex flex-col gap-3">
      <div>
        <div className="mb-1.5 text-[14px] text-rvtx2">Voice</div>
        <div className="w-full rounded-xl border border-rvline bg-rvsurf px-3 py-2.5 text-[15px] text-rvtx">
          {voiceLabel(state)}
        </div>
      </div>
      {body}
      <div className="flex gap-2.5">
        {downloading ? (
          <OutlineButton onClick={() => bridge.cancelDownload()}>Cancel</OutlineButton>
        ) : state.voice_kind !== "piper" && state.has_manifest ? (
          <PrimaryButton onClick={() => bridge.downloadVoice()}>
            {d.state === "failed" ? "Try again" : "Download voice"}
          </PrimaryButton>
        ) : null}
        {state.voice_ready ? (
          inWizard ? (
            <SoftButton onClick={sample}>Play a sample</SoftButton>
          ) : (
            <OutlineButton onClick={sample}>Play a sample</OutlineButton>
          )
        ) : null}
      </div>
    </div>
  );
}
