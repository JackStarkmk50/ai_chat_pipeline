
import { useState } from "react";
import type { Draft, UiState } from "../bridge";
import { bridge } from "../bridge";
import { GhostButton, Header, PrimaryButton, Screen, TextField } from "../components";
import { BridgePill } from "./Home";
import { VoiceBlock } from "./VoiceBlock";

export function Settings({
  state,
  draft,
  saved,
  setDraft,
  close,
}: {
  state: UiState;
  draft: Draft;
  saved: Draft;
  setDraft: (d: Draft) => void;
  close: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);

  return (
    <Screen
      footer={
        <>
          <GhostButton
            onClick={() => {
              setDraft(saved);
              close();
            }}
          >
            Close
          </GhostButton>
          <PrimaryButton
            disabled={!dirty}
            onClick={async () => {
              const err = await bridge.save(draft);
              if (err) setError(err);
              else setError(null);
            }}
          >
            Save
          </PrimaryButton>
        </>
      }
    >
      <Header title="Settings" />
      <div className="text-[15px] font-semibold text-rvtx">Voice</div>
      <VoiceBlock state={state} inWizard={false} />
      <div>
        <div className="mb-1.5 flex items-baseline justify-between">
          <span className="text-[14px] text-rvtx2">Speed</span>
          <span className="text-[14px] text-rvtx2">{draft.speed.toFixed(2).replace(/0$/, "")}x</span>
        </div>
        <input
          type="range"
          min={0.75}
          max={1.5}
          step={0.05}
          value={draft.speed}
          onChange={(e) => setDraft({ ...draft, speed: Number(e.target.value) })}
          className="w-full"
        />
      </div>
      <hr className="border-rvline" />
      <div className="text-[15px] font-semibold text-rvtx">Startup</div>
      <div className="flex items-center justify-between">
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
      <hr className="border-rvline" />
      <div className="text-[15px] font-semibold text-rvtx">Link to your replies</div>
      <TextField
        label="Connection address"
        value={draft.address}
        placeholder="127.0.0.1:47615"
        onChange={(v) => setDraft({ ...draft, address: v })}
      />
      <TextField
        label="Access token, kept separate"
        password
        value={draft.token}
        placeholder="Not set"
        onChange={(v) => setDraft({ ...draft, token: v })}
        action={
          <GhostButton onClick={async () => setDraft({ ...draft, token: await bridge.makeToken() })}>
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
