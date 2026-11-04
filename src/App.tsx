
import { useEffect, useState } from "react";
import type { Draft, UiState } from "./bridge";
import { bridge, mockInitialScreen } from "./bridge";
import { FirstRun } from "./screens/FirstRun";
import { Home } from "./screens/Home";
import { Settings } from "./screens/Settings";

type Screen = { kind: "home" } | { kind: "settings" } | { kind: "first"; step: number };

export default function App() {
  const [state, setState] = useState<UiState | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saved, setSaved] = useState<Draft | null>(null);
  const [screen, setScreen] = useState<Screen>(mockInitialScreen() ?? { kind: "home" });

  useEffect(() => {
    bridge.onState(setState);
    bridge.getState().then((s) => {
      setState(s);
      if (!mockInitialScreen() && !s.first_run_done) setScreen({ kind: "first", step: 1 });
    });
    bridge.getDraft().then((d) => {
      setDraft(d);
      setSaved(d);
    });
  }, []);

  if (!state || !draft || !saved) {
    return <div className="p-5 text-rvtx2">Loading…</div>;
  }

  return (
    <div className="h-full p-5">
      {screen.kind === "first" ? (
        <FirstRun
          state={state}
          draft={draft}
          setDraft={setDraft}
          step={screen.step}
          setStep={(step) => setScreen({ kind: "first", step })}
          done={() => {
            setSaved({ ...draft, first_run_done: true });
            setScreen({ kind: "home" });
          }}
        />
      ) : screen.kind === "settings" ? (
        <Settings
          state={state}
          draft={draft}
          saved={saved}
          setDraft={setDraft}
          close={() => {
            setDraft(saved);
            if (state.first_run_done) setScreen({ kind: "home" });
            else bridge.hideWindow();
          }}
        />
      ) : (
        <Home state={state} openSettings={() => setScreen({ kind: "settings" })} />
      )}
    </div>
  );
}
