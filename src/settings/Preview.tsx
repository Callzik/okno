import { useEffect, useRef } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { SceneRenderer } from "../live/renderer";
import { PHASE_RU, WEATHER_RU, toUpdate, type SceneState } from "../types";

/** Превью: в статичном режиме — картинка обоев, в живом — та же сцена, что на рабочем столе. */
export function Preview({ state }: { state: SceneState | null }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<SceneRenderer | null>(null);
  const showImage = state?.mode === "static" && !!state.image_path;

  useEffect(() => {
    if (showImage || !canvas.current) return;
    const r = new SceneRenderer(canvas.current);
    renderer.current = r;
    const onVis = () => (document.hidden ? r.stop() : r.start());
    document.addEventListener("visibilitychange", onVis);
    r.start();
    return () => {
      r.stop();
      document.removeEventListener("visibilitychange", onVis);
      renderer.current = null;
    };
  }, [showImage]);

  useEffect(() => {
    const u = state && toUpdate(state);
    if (u && renderer.current) renderer.current.setScene(u);
  }, [state, showImage]);

  const title = state?.phase && state.weather
    ? `${PHASE_RU[state.phase]}, ${WEATHER_RU[state.weather]}`
    : "Город не выбран";

  return (
    <div className="preview">
      {showImage ? (
        <img src={convertFileSrc(state!.image_path!)} alt="" />
      ) : (
        <canvas ref={canvas} />
      )}
      <div className="preview-overlay">
        <div className="preview-top">
          {state?.preview && <span className="badge">проверка сцены</span>}
          {state?.offline && <span className="badge warn">нет сети, данные из кэша</span>}
          {state?.paused && <span className="badge">пауза</span>}
        </div>
        <div className="preview-bottom">
          <div>
            <div className="city">{state?.city?.name ?? "Окно"}</div>
            <div className="muted">{title}</div>
          </div>
          {state?.temperature != null && (
            <div className="temp">{Math.round(state.temperature)}°</div>
          )}
        </div>
      </div>
    </div>
  );
}
