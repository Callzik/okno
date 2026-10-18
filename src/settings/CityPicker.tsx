import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { City, SceneState } from "../types";

function label(c: City) {
  return [c.name, c.admin1, c.country].filter(Boolean).join(", ");
}

export function CityPicker({ onState, onError }: {
  onState: (s: SceneState) => void;
  onError: (m: string | null) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<City[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (query.trim().length < 2) {
      setResults([]);
      return;
    }
    const id = setTimeout(() => {
      invoke<City[]>("search_city", { query })
        .then((r) => setResults(r))
        .catch((e) => onError(String(e)));
    }, 350);
    return () => clearTimeout(id);
  }, [query, onError]);

  async function choose(city: City) {
    setQuery("");
    setResults([]);
    onState(await invoke<SceneState>("set_city", { city }));
  }

  async function detect() {
    setBusy(true);
    try {
      const city = await invoke<City>("detect_city");
      await choose(city);
      onError(null);
    } catch (e) {
      onError(`${e}. Выберите город вручную.`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="city-picker">
      <div className="row">
        <input
          placeholder="Найти город…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button onClick={detect} disabled={busy} title="Определить по IP">
          {busy ? "…" : "Авто"}
        </button>
      </div>
      {results.length > 0 && (
        <ul className="results">
          {results.map((c) => (
            <li key={`${c.latitude},${c.longitude}`} onClick={() => choose(c)}>
              {label(c)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
