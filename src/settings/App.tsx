import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Preview } from "./Preview";
import { CityPicker } from "./CityPicker";
import { PHASES, PHASE_RU, WEATHERS, WEATHER_RU, type Mode, type SceneState } from "../types";

export function App() {
  const [state, setState] = useState<SceneState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [autostart, setAutostart] = useState(false);

  const onError = useCallback((m: string | null) => setError(m), []);
  const run = useCallback(async (p: Promise<SceneState>) => {
    try {
      setState(await p);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    run(invoke<SceneState>("get_state"));
    isEnabled().then(setAutostart).catch(() => {});
    const un = [
      listen<SceneState>("state-changed", (e) => {
        setState(e.payload);
        if (!e.payload.offline) setError(null);
      }),
      listen<{ message: string }>("weather-error", (e) => setError(e.payload.message)),
    ];
    return () => un.forEach((p) => p.then((f) => f()));
  }, [run]);

  const setMode = (mode: Mode) => run(invoke("set_mode", { mode }));
  const preview = (scene: string) => run(invoke("preview_scene", { scene: scene || null }));
  const pickFolder = async () => {
    const folder = await open({ directory: true, title: "Папка с картинками сцен" });
    if (typeof folder === "string") run(invoke("set_custom_folder", { folder }));
  };
  const toggleAutostart = async () => {
    try {
      if (autostart) await disable();
      else await enable();
      setAutostart(await isEnabled());
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <main>
      <Preview state={state} />

      {error && <div className="alert">{error}</div>}
      {state?.warning && <div className="alert">{state.warning}</div>}

      <section>
        <CityPicker onState={setState} onError={onError} />
      </section>

      <section className="sun">
        <div><span className="muted">Восход</span> {state?.sunrise ?? "—"}</div>
        <div><span className="muted">Закат</span> {state?.sunset ?? "—"}</div>
        <div><span className="muted">Обновлено</span> {state?.updated_at ?? "—"}</div>
      </section>

      <section>
        <div className="segmented">
          {(["static", "live"] as Mode[]).map((m) => (
            <button
              key={m}
              className={state?.mode === m ? "active" : ""}
              onClick={() => setMode(m)}
            >
              {m === "static" ? "Статичный" : "Живой"}
            </button>
          ))}
        </div>
      </section>

      <section className="settings">
        <label className="field">
          <span>Проверить сцену</span>
          <select value={state?.preview ?? ""} onChange={(e) => preview(e.target.value)}>
            <option value="">Реальная погода</option>
            {PHASES.map((p) => (
              <optgroup key={p} label={PHASE_RU[p]}>
                {WEATHERS.map((w) => (
                  <option key={w} value={`${p}_${w}`}>
                    {PHASE_RU[p]}, {WEATHER_RU[w]}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
        </label>

        <div className="field">
          <span>Своя папка с картинками</span>
          <div className="row">
            <button onClick={pickFolder}>Выбрать</button>
            {state?.custom_folder && (
              <button onClick={() => run(invoke("set_custom_folder", { folder: null }))}>
                Сбросить
              </button>
            )}
          </div>
        </div>
        {state?.custom_folder && <div className="path muted">{state.custom_folder}</div>}

        <label className="field check">
          <span>Запускать вместе с Windows</span>
          <input type="checkbox" checked={autostart} onChange={toggleAutostart} />
        </label>
      </section>
    </main>
  );
}
