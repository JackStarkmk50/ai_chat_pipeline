import { useState } from "react";
import type { Draft, UiState } from "../bridge";
import { bridge } from "../bridge";
import {
  Card,
  GhostButton,
  Header,
  Pill,
  PrimaryButton,
  Screen,
  StepDots,
  TextField,
} from "../components";
import { BridgePill } from "./Home";
import { VoiceBlock } from "./VoiceBlock";

export function linkOk(d: Draft): boolean {
  return (
    /^(127\.\d{1,3}\.\d{1,3}\.\d{1,3}|localhost|\[?::1\]?):\d+$/.test(d.address.trim()) &&
    d.token.length > 0
  );
}

export function FirstRun({
  state,
  draft,
  setDraft,
  step,
  setStep,
  done,
}: {
  state: UiState;
  draft: Draft;
  setDraft: (d: Draft) => void;
  step: number;
  setStep: (n: number) => void;
  done: () => void;
}) {
  const [error, setError] = useState<string | null>(null);

  const footer = (left: React.ReactNode, right: React.ReactNode) => (
    <>
      {left}
      {right}
    </>
  );

  if (step === 1) {
    return (
      <Screen
        footer={footer(
          state.voice_kind === "piper" ? (
            <GhostButton onClick={() => setStep(0)}>Back</GhostButton>
          ) : (
            <GhostButton onClick={() => setStep(2)}>Skip for now</GhostButton>
          ),
          <PrimaryButton onClick={() => setStep(2)}>Next</PrimaryButton>,
        )}
      >
        <Header title="Reply Voice" />
        <StepDots step={1} />
        <div>
          <div className="text-[24px] font-semibold text-rvtx">
            {state.download.state === "downloading"
              ? "Getting the voice"
              : state.voice_kind === "piper"
                ? "Voice is ready"
                : "Get the voice"}
          </div>
          <div className="mt-1.5 text-[15px] text-rvtx2">
            {state.download.state === "downloading"
              ? "Keep this window open. You can still skip and finish later."
              : state.voice_kind === "piper"
                ? "Play a sample to check the volume and speed."
                : "Reply Voice reads each reply aloud with a voice that runs on this computer. English first."}
          </div>
        </div>
        <VoiceBlock state={state} inWizard />
      </Screen>
    );
  }

  if (step === 2) {
    return (
      <Screen
        footer={footer(
          <GhostButton onClick={() => setStep(1)}>Back</GhostButton>,
          <PrimaryButton
            onClick={async () => {
              const err = await bridge.validate(draft);
              if (err) setError(err);
              else {
                setError(null);
                setStep(3);
              }
            }}
          >
            Next
          </PrimaryButton>,
        )}
      >
        <Header title="Reply Voice" />
        <StepDots step={2} />
        <div>
          <div className="text-[24px] font-semibold text-rvtx">Link to your replies</div>
          <div className="mt-1.5 text-[15px] text-rvtx2">
            Reply Voice needs a route to receive each reply. The address and the access token are
            two separate things.
          </div>
        </div>
        <TextField
          label="Connection address"
          value={draft.address}
          placeholder="127.0.0.1:47615"
          onChange={(v) => setDraft({ ...draft, address: v })}
        />
        <TextField
          label="Access token"
          password
          value={draft.token}
          placeholder="Not set"
          onChange={(v) => setDraft({ ...draft, token: v })}
          action={
            <GhostButton
              onClick={async () => setDraft({ ...draft, token: await bridge.makeToken() })}
            >
              Make one
            </GhostButton>
          }
        />
        <div className="text-[13px] text-rvtx2">
          The token is hidden. Anything that sends replies here must present it.
        </div>
        <BridgePill state={state} />
        {error ? <div className="text-[13px] text-rvwarn">{error}</div> : null}
      </Screen>
    );
  }

  const voiceOk = state.voice_ready;
  const linkReady = linkOk(draft);
  const all = voiceOk && linkReady;
  return (
    <Screen
      footer={footer(
        <GhostButton onClick={() => setStep(2)}>Back</GhostButton>,
        <PrimaryButton
          onClick={async () => {
            const err = await bridge.save({ ...draft, first_run_done: true });
            if (err) setError(err);
            else done();
          }}
        >
          {all ? "Done" : "Finish setup"}
        </PrimaryButton>,
      )}
    >
      <Header title="Reply Voice" />
      <StepDots step={3} />
      <div>
        <div className="text-[24px] font-semibold text-rvtx">
          {all ? "Ready to listen" : "Almost ready"}
        </div>
        <div className="mt-1.5 text-[15px] text-rvtx2">
          {all
            ? "Replies that reach this app on this computer will be read aloud. No reply source is connected yet."
            : "Two things are still open. The app will not read anything until both are done."}
        </div>
      </div>
      <Card>
        <div className="divide-y divide-rvline">
          <div className="flex items-center gap-3 py-2.5 first:pt-0">
            <Pill kind={voiceOk ? "ok" : "warn"}>{voiceOk ? "Done" : "To do"}</Pill>
            <div>
              <div className="text-[15px] font-medium text-rvtx">Voice</div>
              <div className="text-[13px] text-rvtx2">
                {voiceOk ? "On this computer" : "Not on this computer yet"}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-3 py-2.5">
            <Pill kind={linkReady ? "ok" : "warn"}>{linkReady ? "Done" : "To do"}</Pill>
            <div>
              <div className="text-[15px] font-medium text-rvtx">Link to your replies</div>
              <div className="text-[13px] text-rvtx2">
                {linkReady ? "Address and token set" : "Bridge not connected"}
              </div>
            </div>
          </div>
          <div className="flex items-center justify-between py-2.5 last:pb-0">
            <div>
              <div className="text-[15px] font-medium text-rvtx">Start with the computer</div>
              <div className="text-[13px] text-rvtx2">Off until you turn it on</div>
            </div>
            <button
              type="button"
              onClick={() => setDraft({ ...draft, start_with_computer: !draft.start_with_computer })}
              className={`rounded-xl border px-4 py-1.5 text-[14px] font-medium ${
                draft.start_with_computer
                  ? "border-rvor bg-rvor text-rvorink"
                  : "border-rvedge text-rvtx"
              }`}
            >
              {draft.start_with_computer ? "On" : "Off"}
            </button>
          </div>
        </div>
      </Card>
      <BridgePill state={state} />
      {error ? <div className="text-[13px] text-rvwarn">{error}</div> : null}
    </Screen>
  );
}
