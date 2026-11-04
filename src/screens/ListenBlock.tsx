import type { UiState } from "../bridge";
import { bridge } from "../bridge";
import { Card, OutlineButton, Pill, PrimaryButton } from "../components";

export function ListenBlock({
  state,
  openSettings,
}: {
  state: UiState;
  openSettings: () => void;
}) {
  if (!state.listen_supported) return null;
  const phase = state.listen_phase;
  const recording = phase === "recording";
  const transcribing = phase === "transcribing";

  if (!state.stt_model_ready) {
    return (
      <Card>
        <div className="text-[14px] font-medium text-rvtx">Dictation</div>
        <div className="mt-0.5 text-[13px] text-rvtx2">
          Talk and see the words here. They stay on this computer. A speech
          model is needed first.
        </div>
        <div className="mt-2.5">
          <OutlineButton onClick={openSettings}>Set up in Settings</OutlineButton>
        </div>
      </Card>
    );
  }

  const copyAll = () => navigator.clipboard.writeText(state.transcript.join("\n"));

  return (
    <Card>
      <div className="flex items-center justify-between gap-2">
        <div className="text-[14px] font-medium text-rvtx">Dictation</div>
        {recording ? <Pill kind="ok">Recording</Pill> : null}
        {transcribing ? <Pill kind="note">Transcribing…</Pill> : null}
      </div>
      <div className="mt-0.5 text-[13px] text-rvtx2">
        {recording
          ? "Listening through the microphone. Stop when you're done."
          : transcribing
            ? "Turning your speech into text…"
            : `Uses ${state.stt_model_label} on this computer. Transcription runs locally - nothing is sent automatically.`}
      </div>
      {state.transcript.length > 0 ? (
        <div className="mt-2 max-h-40 overflow-y-auto rounded-lg bg-rvline/30 p-2.5 text-[14px] leading-relaxed text-rvtx">
          {state.transcript.map((line, i) => (
            <p key={i}>{line}</p>
          ))}
        </div>
      ) : null}
      {state.listen_error ? (
        <div className="mt-1.5 text-[13px] text-rvwarn">{state.listen_error}</div>
      ) : null}
      <div className="mt-2.5 flex gap-2.5">
        {recording ? (
          <OutlineButton onClick={() => bridge.listenStop()}>Stop</OutlineButton>
        ) : (
          <PrimaryButton
            disabled={transcribing}
            onClick={async () => bridge.listenStart()}
          >
            Record
          </PrimaryButton>
        )}
        {state.transcript.length > 0 && !recording && !transcribing ? (
          <OutlineButton onClick={copyAll}>Copy</OutlineButton>
        ) : null}
      </div>
    </Card>
  );
}
