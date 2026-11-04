import type { UiState } from "../bridge";
import { bridge } from "../bridge";
import { GhostButton, Header, OutlineButton, Pill, Screen } from "../components";

export function BridgePill({ state }: { state: UiState }) {
  if (state.link_error) return <Pill kind="warn">Link problem: {state.link_error}</Pill>;
  if (state.bridge === "received") return <Pill kind="ok">Reply received</Pill>;
  if (state.bridge === "waiting") return <Pill kind="note">Listening on this computer</Pill>;
  return <Pill kind="warn">Bridge not connected</Pill>;
}

export function Home({ state, openSettings }: { state: UiState; openSettings: () => void }) {
  const linked = state.bridge !== "not_connected" || !!state.link_error;
  const heading = state.speaking
    ? "Reading a reply aloud…"
    : linked
      ? "Listening on this computer."
      : "Nothing can reach this app yet.";
  const subtext = state.speaking
    ? "The newest reply is playing."
    : linked
      ? "Replies that reach this app on this computer will be read aloud. No reply source is connected yet."
      : "Replies will not play until a route to this app is set up and confirmed.";

  return (
    <Screen
      footer={
        <>
          <GhostButton onClick={openSettings}>Settings</GhostButton>
          <OutlineButton
            disabled={!state.can_replay || !state.voice_ready}
            onClick={() => bridge.replay()}
          >
            Replay
          </OutlineButton>
        </>
      }
    >
      <Header title="Reply Voice" />
      <div className="flex flex-wrap gap-2">
        <BridgePill state={state} />
        {!state.voice_ready ? <Pill kind="warn">Voice not ready</Pill> : null}
        {state.last_error ? <Pill kind="warn">Could not read this reply</Pill> : null}
      </div>
      <div className="mt-2">
        <div className="text-[24px] font-semibold leading-snug text-rvtx">{heading}</div>
        <div className="mt-1.5 text-[15px] text-rvtx2">{subtext}</div>
        {state.last_error ? (
          <div className="mt-1.5 text-[13px] text-rvwarn">{state.last_error}</div>
        ) : null}
      </div>
    </Screen>
  );
}
