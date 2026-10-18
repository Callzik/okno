import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SceneRenderer } from "./renderer";
import { toUpdate, type SceneState, type SceneUpdate } from "../types";

const canvas = document.getElementById("scene") as HTMLCanvasElement;
const renderer = new SceneRenderer(canvas);


function setPaused(p: boolean) {

  if (p) renderer.stop();
  else renderer.start();
}

window.addEventListener("resize", () => renderer.resize());

listen<SceneUpdate>("scene-update", (e) => renderer.setScene(e.payload));
listen<{ paused: boolean }>("live-pause", (e) => setPaused(e.payload.paused));

invoke<SceneState>("get_state").then((s) => {
  const u = toUpdate(s);
  if (u) renderer.setScene(u);
  setPaused(s.paused);
});

