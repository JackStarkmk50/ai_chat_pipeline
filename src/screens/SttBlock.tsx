import type { Draft, UiState } from "../bridge";
import { bridge } from "../bridge";
import { Card, OutlineButton, Pill, PrimaryButton, ProgressBar } from "../components";

export function SttBlock({
  state,
  draft,
  setDraft,
}: {
  state: UiState;
  draft: Draft;
  setDraft: (d: Draft) => void;
}) {
  if (!state.listen_supported) return null;
  const d = state.stt_download;
  const selected =
    state.stt_models.find((m) => m.id === draft.stt_model) ?? state.stt_models[0];

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
        <div className="text-[14px] font-medium text-rvwarn">
          The speech model could not be downloaded
        </div>
        <div className="mt-0.5 text-[13px] text-rvtx2">{d.message}</div>
      </Card>
    );
  } else if (!state.stt_has_manifest) {
    body = (
      <Card warn>
        <div className="text-[14px] font-medium text-rvwarn">No speech model list</div>
        <div className="mt-0.5 text-[13px] text-rvtx2">
          Dictation needs the model list that ships with the app.
        </div>
      </Card>
    );
  } else if (state.stt_model_ready) {
    body = (
      <Card>
        <div className="flex items-center gap-2.5">
          <Pill kind="ok">Ready</Pill>
          <span className="text-[14px] text-rvtx2">On this computer</span>
        </div>
      </Card>
    );
  } else {
    body = (
      <Card>
        <div className="text-[14px] font-medium text-rvtx">No speech model yet</div>
        <div className="mt-0.5 text-[13px] text-rvtx2">
          Download one (about {selected ? selected.mb : 0} MB, once). Until then
          Record cannot start.
        </div>
      </Card>
    );
  }

  const downloading = d.state === "downloading";
  return (
    <div className="flex flex-col gap-3">
      {state.stt_models.length > 0 ? (
        <div>
          <div className="mb-1.5 text-[14px] text-rvtx2">Speech model</div>
          <select
            value={selected?.id ?? ""}
            onChange={(e) => setDraft({ ...draft, stt_model: e.target.value })}
            className="w-full rounded-xl border border-rvline bg-rvsurf px-3 py-2.5 text-[15px] text-rvtx outline-none focus:border-rvedge"
          >
            {state.stt_models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.label} · {m.mb} MB{m.installed ? " · on this computer" : ""}
              </option>
            ))}
          </select>
          {selected ? (
            <div className="mt-1 text-[13px] text-rvtx2">
              {selected.description} Licence: {selected.license}.
            </div>
          ) : null}
        </div>
      ) : null}
      {body}
      <div className="flex gap-2.5">
        {downloading ? (
          <OutlineButton onClick={() => bridge.cancelSttDownload()}>Cancel</OutlineButton>
        ) : selected && !selected.installed ? (
          <PrimaryButton onClick={() => bridge.downloadStt(selected.id)}>
            {d.state === "failed" ? "Try again" : "Download model"}
          </PrimaryButton>
        ) : null}
      </div>
    </div>
  );
}
