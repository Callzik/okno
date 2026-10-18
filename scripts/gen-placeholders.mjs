// Генерирует заглушки-градиенты для всех 24 сцен: src-tauri/assets/scenes/{phase}_{weather}.jpg
// Запуск: npm run scenes  (существующие файлы не перезаписываются, --force перезапишет)
import { mkdirSync, existsSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import jpeg from "jpeg-js";

const W = 1920, H = 1080, HORIZON = H * 0.78;
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "assets", "scenes");
const force = process.argv.includes("--force");
mkdirSync(out, { recursive: true });

const PHASES = {
  night: { top: [6, 9, 26], bottom: [18, 24, 58], sun: null, moon: true },
  dawn: { top: [40, 58, 118], bottom: [238, 140, 140], sun: [0.3, 0.74, [255, 190, 140]] },
  day: { top: [34, 112, 210], bottom: [150, 200, 242], sun: [0.62, 0.22, [255, 248, 225]] },
  sunset: { top: [46, 54, 112], bottom: [246, 132, 66], sun: [0.7, 0.72, [255, 160, 80]] },
};
const WEATHER = {
  clear: { dull: 0, dark: 1 },
  cloudy: { dull: 0.5, dark: 0.9, clouds: 10 },
  fog: { dull: 0.75, dark: 1.05, fog: true },
  rain: { dull: 0.7, dark: 0.72, clouds: 12, rain: 900 },
  snow: { dull: 0.6, dark: 0.95, clouds: 10, snow: 900 },
  storm: { dull: 0.85, dark: 0.5, clouds: 14, rain: 1400, bolt: true },
};

const lerp = (a, b, t) => a + (b - a) * t;
const mix = (a, b, t) => a.map((v, i) => lerp(v, b[i], t));
const grey = (c) => { const l = (c[0] + c[1] + c[2]) / 3; return [l, l, l]; };
function rng(seed) {
  return () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
}

function render(phaseName, weatherName) {
  const p = PHASES[phaseName], w = WEATHER[weatherName];
  const px = new Float32Array(W * H * 3);
  const tone = (c) => mix(c, grey(c), w.dull).map((v) => v * w.dark);
  const top = tone(p.top), bottom = tone(p.bottom);
  const set = (x, y, c, a = 1) => {
    if (x < 0 || y < 0 || x >= W || y >= H) return;
    const i = ((y | 0) * W + (x | 0)) * 3;
    for (let k = 0; k < 3; k++) px[i + k] = lerp(px[i + k], c[k], a);
  };

  // Небо.
  for (let y = 0; y < H; y++) {
    const c = mix(top, bottom, Math.min(1, y / HORIZON));
    for (let x = 0; x < W; x++) { const i = (y * W + x) * 3; px[i] = c[0]; px[i + 1] = c[1]; px[i + 2] = c[2]; }
  }
  const r = rng(7);
  // Звёзды.
  if (phaseName === "night" && (weatherName === "clear" || weatherName === "cloudy")) {
    for (let i = 0; i < 500; i++) set(r() * W, r() * HORIZON * 0.9, [255, 255, 255], 0.3 + r() * 0.6);
  }
  // Солнце или луна со свечением.
  const vis = weatherName === "clear" ? 1 : weatherName === "cloudy" ? 0.6 : 0.15;
  const disc = p.sun ? { x: p.sun[0] * W, y: p.sun[1] * H, c: p.sun[2], rad: 38 } : { x: 0.28 * W, y: 0.2 * H, c: [236, 238, 250], rad: 30 };
  for (let y = Math.max(0, disc.y - 320); y < Math.min(HORIZON, disc.y + 320); y++) {
    for (let x = Math.max(0, disc.x - 320); x < Math.min(W, disc.x + 320); x++) {
      const d = Math.hypot(x - disc.x, y - disc.y);
      let a = d < disc.rad ? 1 : Math.max(0, 1 - d / 320) ** 2 * (p.sun ? 0.5 : 0.15);
      if (!p.sun && d < disc.rad && Math.hypot(x - disc.x - 14, y - disc.y + 6) < disc.rad * 0.9) a = 0;
      set(x, y, disc.c, a * vis);
    }
  }
  // Облака — размытые пятна.
  for (let n = 0; n < (w.clouds ?? 0); n++) {
    const cx = r() * W, cy = r() * HORIZON * 0.55, rad = 120 + r() * 200;
    const col = mix(mix(top, [255, 255, 255], 0.45), [70, 74, 84], w.rain ? 0.6 : 0);
    for (let k = 0; k < 6; k++) {
      const bx = cx + (r() - 0.5) * rad * 2, by = cy + (r() - 0.5) * rad * 0.4, br = rad * (0.4 + r() * 0.4);
      for (let y = Math.max(0, by - br); y < Math.min(H, by + br); y += 1)
        for (let x = Math.max(0, bx - br); x < Math.min(W, bx + br); x += 1) {
          const d = Math.hypot(x - bx, y - by) / br;
          if (d < 1) set(x, y, col, (1 - d) ** 2 * 0.35);
        }
    }
  }
  // Город: два ряда домов, ночью светятся окна.
  const night = phaseName === "night";
  const light = phaseName === "day" ? 0.5 : phaseName === "night" ? 0 : 0.25;
  const base = mix([8, 10, 22], mix(top, [40, 46, 60], 0.6), light);
  const rc = rng(42);
  for (const row of [0, 1]) {
    let x = -rc() * 40;
    const color = mix(base, mix(top, [255, 255, 255], 0.1), row === 0 ? 0.35 : 0);
    while (x < W) {
      const bw = (40 + rc() * 90) * (row === 0 ? 0.8 : 1);
      // Дальний ряд выше и светлее, ближний ниже и темнее.
      const y0 = row === 0 ? H * (0.56 + rc() * 0.24) : H * (0.72 + rc() * 0.2);
      for (let y = Math.max(0, y0); y < H; y++) for (let xx = x; xx < x + bw; xx++) set(xx, y, color);
      if (row === 1 && (night || phaseName !== "day")) {
        for (let wy = y0 + 12; wy < H - 6; wy += 14)
          for (let wx = x + 6; wx < x + bw - 6; wx += 13)
            if (rc() < (night ? 0.22 : 0.08)) {
              const wc = rc() < 0.8 ? [255, 206, 120] : [170, 210, 255];
              for (let yy = wy; yy < wy + 7; yy++) for (let xx = wx; xx < wx + 6; xx++) set(xx, yy, wc, 0.85);
            }
      }
      x += bw + rc() * 6;
    }
  }
  // Туман.
  if (w.fog) {
    for (let y = 0; y < H; y++) {
      const a = 0.18 + 0.45 * Math.exp(-(((y - HORIZON + 120) / 220) ** 2));
      for (let x = 0; x < W; x++) set(x, y, [205, 210, 218], a);
    }
  }
  // Дождь.
  for (let n = 0; n < (w.rain ?? 0); n++) {
    const x = r() * W * 1.1, y = r() * H, len = 14 + r() * 18;
    for (let t = 0; t < len; t++) set(x - t * 0.15, y + t, [190, 205, 230], 0.4);
  }
  // Снег.
  for (let n = 0; n < (w.snow ?? 0); n++) {
    const x = r() * W, y = r() * H, rad = 1 + r() * 2.5;
    for (let yy = -rad; yy <= rad; yy++) for (let xx = -rad; xx <= rad; xx++)
      if (xx * xx + yy * yy <= rad * rad) set(x + xx, y + yy, [255, 255, 255], 0.85);
  }
  // Молния.
  if (w.bolt) {
    let x = W * 0.58, y = 0;
    while (y < HORIZON - 80) {
      const nx = x + (r() - 0.5) * 70, ny = y + 20 + r() * 30;
      for (let t = 0; t < 1; t += 0.02) {
        const bx = lerp(x, nx, t), by = lerp(y, ny, t);
        for (let o = -2; o <= 2; o++) set(bx + o, by, [235, 240, 255], o === 0 ? 1 : 0.5);
      }
      x = nx; y = ny;
    }
  }

  const data = Buffer.alloc(W * H * 4);
  for (let i = 0, j = 0; i < px.length; i += 3, j += 4) {
    data[j] = px[i]; data[j + 1] = px[i + 1]; data[j + 2] = px[i + 2]; data[j + 3] = 255;
  }
  return jpeg.encode({ data, width: W, height: H }, 88).data;
}

let made = 0;
for (const phase of Object.keys(PHASES)) {
  for (const weather of Object.keys(WEATHER)) {
    const file = join(out, `${phase}_${weather}.jpg`);
    if (existsSync(file) && !force) continue;
    writeFileSync(file, render(phase, weather));
    made++;
  }
}
console.log(`Готово: ${made} новых картинок в ${out}`);
