// Живая сцена на Canvas 2D: небо, солнце/луна, звёзды, облака, город, осадки, туман, молнии.
// Статичные слои рисуются заранее в offscreen-холсты, каждый кадр — только движение.

import { convertFileSrc } from "@tauri-apps/api/core";
import type { SceneUpdate } from "../types";

type RGB = [number, number, number];

const MAX_FPS = 30;

// Цвет неба (верх, низ) по высоте солнца.
const SKY: { h: number; top: RGB; bottom: RGB }[] = [
  { h: -18, top: [4, 6, 18], bottom: [10, 14, 36] },
  { h: -9, top: [10, 18, 50], bottom: [30, 34, 80] },
  { h: -4, top: [28, 40, 92], bottom: [190, 92, 80] },
  { h: 0, top: [48, 72, 140], bottom: [244, 140, 78] },
  { h: 4, top: [62, 106, 178], bottom: [246, 188, 120] },
  { h: 10, top: [56, 122, 212], bottom: [160, 204, 240] },
  { h: 40, top: [30, 110, 210], bottom: [142, 196, 242] },
];

const mix = (a: RGB, b: RGB, t: number): RGB => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
const css = (c: RGB, a = 1) =>
  `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${a})`;
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

function skyColors(s: SceneUpdate): [RGB, RGB] {
  const h = clamp(s.sun_elevation, -18, 40);
  let i = 0;
  while (i < SKY.length - 2 && h > SKY[i + 1].h) i++;
  const a = SKY[i], b = SKY[i + 1];
  const t = clamp((h - a.h) / (b.h - a.h), 0, 1);
  let top = mix(a.top, b.top, t);
  let bottom = mix(a.bottom, b.bottom, t);
  // Рассвет розовее заката.
  if (s.phase === "dawn") bottom = mix(bottom, [236, 130, 150], 0.25);
  // Облачность и осадки гасят цвет.
  const grey = (c: RGB): RGB => {
    const l = (c[0] + c[1] + c[2]) / 3;
    return [l, l, l];
  };
  const cc = clamp(s.cloud_cover / 100, 0, 1);
  const dull = { clear: 0, cloudy: 0.55, fog: 0.75, rain: 0.7, snow: 0.6, storm: 0.85 }[s.weather] * Math.max(cc, 0.5);
  top = mix(top, grey(top), dull);
  bottom = mix(bottom, grey(bottom), dull);
  const dark = { clear: 1, cloudy: 0.9, fog: 1.05, rain: 0.75, snow: 0.95, storm: 0.55 }[s.weather];
  top = top.map((v) => v * dark) as RGB;
  bottom = bottom.map((v) => v * dark) as RGB;
  return [top, bottom];
}

// Детерминированный генератор, чтобы город и звёзды не прыгали при пересчёте.
function rng(seed: number) {
  return () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 4294967296;
  };
}

interface Cloud { x: number; y: number; speed: number; sprite: HTMLCanvasElement }
interface Drop { x: number; y: number; v: number; len: number }
interface Flake { x: number; y: number; v: number; r: number; p: number }

export class SceneRenderer {
  private ctx: CanvasRenderingContext2D;
  private sky = document.createElement("canvas");
  private city = document.createElement("canvas");
  private scene: SceneUpdate | null = null;
  private clouds: Cloud[] = [];
  private drops: Drop[] = [];
  private flakes: Flake[] = [];
  private twinkle: { x: number; y: number; p: number }[] = [];
  private flash = 0;
  private nextFlash = 0;
  private raf = 0;
  private running = false;
  private last = 0;
  private w = 0;
  private h = 0;
  private horizon = 0;
  /** Фото сцены. Если загружено, рисуется вместо нарисованного неба и города. */
  private photo: HTMLImageElement | null = null;
  private photoPath: string | null = null;

  constructor(private canvas: HTMLCanvasElement, private scale = 1) {
    this.ctx = canvas.getContext("2d", { alpha: false })!;
    this.resize();
  }

  resize() {
    const r = this.canvas.getBoundingClientRect();
    this.w = Math.max(1, Math.round((r.width || innerWidth) * this.scale));
    this.h = Math.max(1, Math.round((r.height || innerHeight) * this.scale));
    this.canvas.width = this.w;
    this.canvas.height = this.h;
    this.horizon = this.h * 0.78;
    if (this.scene) this.setScene(this.scene);
  }

  setScene(s: SceneUpdate) {
    const changed =
      !this.scene || this.scene.weather !== s.weather || this.scene.phase !== s.phase ||
      Math.abs(this.scene.cloud_cover - s.cloud_cover) > 5;
    this.scene = s;
    this.loadPhoto(s.image_path ?? null);
    this.drawSky();
    this.drawCity();
    if (changed || this.clouds.length === 0) this.spawn();
    this.frame(performance.now(), 0);
  }

  private loadPhoto(path: string | null) {
    if (path === this.photoPath) return;
    this.photoPath = path;
    if (!path) {
      this.photo = null;
      return;
    }
    const img = new Image();
    img.onload = () => {
      if (this.photoPath !== path) return;
      this.photo = img;
      this.drawSky();
      this.drawCity();
      this.spawn();
      this.frame(performance.now(), 0);
    };
    img.onerror = () => {
      if (this.photoPath === path) this.photo = null;
    };
    img.src = convertFileSrc(path);
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.last = performance.now();
    const loop = (t: number) => {
      if (!this.running) return;
      this.raf = requestAnimationFrame(loop);
      const dt = t - this.last;
      if (dt < 1000 / MAX_FPS - 2) return;
      this.last = t;
      this.frame(t, Math.min(dt, 100) / 1000);
    };
    this.raf = requestAnimationFrame(loop);
  }

  stop() {
    this.running = false;
    cancelAnimationFrame(this.raf);
  }

  private drawSky() {
    const s = this.scene!;
    const { w, h } = this;
    this.sky.width = w;
    this.sky.height = h;
    const c = this.sky.getContext("2d")!;
    this.twinkle = [];
    if (this.photo) {
      const p = this.photo;
      const k = Math.max(w / p.naturalWidth, h / p.naturalHeight);
      const pw = p.naturalWidth * k, ph = p.naturalHeight * k;
      c.drawImage(p, (w - pw) / 2, (h - ph) / 2, pw, ph);
      return;
    }
    const [top, bottom] = skyColors(s);
    const g = c.createLinearGradient(0, 0, 0, this.horizon);
    g.addColorStop(0, css(top));
    g.addColorStop(1, css(bottom));
    c.fillStyle = g;
    c.fillRect(0, 0, w, h);

    const cc = clamp(s.cloud_cover / 100, 0, 1);
    const clearness = s.weather === "clear" || s.weather === "cloudy" ? 1 - cc * 0.8 : 0.1;

    // Звёзды.
    const starAlpha = clamp((-s.sun_elevation - 4) / 8, 0, 1) * clearness;
    if (starAlpha > 0.05) {
      const r = rng(7);
      const n = Math.round((w * h) / 9000);
      c.fillStyle = "#fff";
      for (let i = 0; i < n; i++) {
        const x = r() * w, y = r() * this.horizon * 0.9, size = r() * 1.3 + 0.3;
        c.globalAlpha = starAlpha * (0.3 + r() * 0.7);
        c.fillRect(x, y, size, size);
        if (i % 12 === 0) this.twinkle.push({ x, y, p: r() * 6.28 });
      }
      c.globalAlpha = 1;
    }

    // Солнце над горизонтом / луна ночью.
    const visible = s.weather === "clear" ? 1 : s.weather === "cloudy" ? 1 - cc * 0.7 : 0.15;
    if (s.sun_elevation > -4) {
      const sx = w * (s.phase === "dawn" ? 0.3 : s.phase === "sunset" ? 0.7 : 0.62);
      const sy = this.horizon - (clamp(s.sun_elevation, -4, 50) / 50) * this.horizon * 0.85;
      const rad = Math.min(w, h) * 0.035;
      const warm: RGB = s.sun_elevation < 8 ? [255, 170, 90] : [255, 244, 214];
      const glow = c.createRadialGradient(sx, sy, rad * 0.5, sx, sy, rad * 8);
      glow.addColorStop(0, css(warm, 0.55 * visible));
      glow.addColorStop(1, css(warm, 0));
      c.fillStyle = glow;
      c.fillRect(0, 0, w, h);
      c.fillStyle = css(warm, visible);
      c.beginPath();
      c.arc(sx, sy, rad, 0, Math.PI * 2);
      c.fill();
    } else {
      const mx = w * 0.28, my = h * 0.2, rad = Math.min(w, h) * 0.028;
      const glow = c.createRadialGradient(mx, my, rad, mx, my, rad * 6);
      glow.addColorStop(0, `rgba(200,210,255,${0.18 * visible})`);
      glow.addColorStop(1, "rgba(200,210,255,0)");
      c.fillStyle = glow;
      c.fillRect(0, 0, w, h);
      c.drawImage(moonSprite(rad, 0.9 * visible), mx - rad, my - rad);
    }
  }

  private drawCity() {
    const s = this.scene!;
    const { w, h } = this;
    this.city.width = w;
    this.city.height = h;
    const c = this.city.getContext("2d")!;
    if (this.photo) return;
    const [top] = skyColors(s);
    const light = clamp((s.sun_elevation + 8) / 30, 0, 1);
    const base: RGB = mix([8, 10, 22], mix(top, [40, 46, 60], 0.6), light * 0.5);
    const night = s.sun_elevation < 2;
    const r = rng(42);
    // Два ряда домов: дальний светлее.
    for (const row of [0, 1]) {
      let x = -r() * 40 * this.scale;
      const color = mix(base, mix(top, [255, 255, 255], 0.1), row === 0 ? 0.35 : 0);
      while (x < w) {
        const bw = (40 + r() * 90) * this.scale * (row === 0 ? 0.8 : 1);
        // Дальний ряд выше и светлее, ближний ниже и темнее.
        const y = row === 0 ? h * (0.56 + r() * 0.24) : h * (0.72 + r() * 0.2);
        c.fillStyle = css(color);
        c.fillRect(x, y, bw, h);
        if (row === 1 && night) {
          const ws = 6 * this.scale;
          for (let wy = y + ws * 2; wy < h - ws; wy += ws * 2.4) {
            for (let wx = x + ws; wx < x + bw - ws; wx += ws * 2.2) {
              if (r() < 0.22) {
                c.fillStyle = r() < 0.8 ? "rgba(255,206,120,0.85)" : "rgba(170,210,255,0.8)";
                c.fillRect(wx, wy, ws, ws * 1.2);
              }
            }
          }
        }
        x += bw + r() * 6 * this.scale;
      }
    }
  }

  private spawn() {
    const s = this.scene!;
    const { w, h } = this;
    const r = Math.random;
    const cc = clamp(s.cloud_cover / 100, 0, 1);
    const [top] = skyColors(s);
    const lightness = clamp((s.sun_elevation + 6) / 20, 0.15, 1);
    const cloudColor = mix(mix(top, [255, 255, 255], 0.5), [70, 74, 84], s.weather === "storm" || s.weather === "rain" ? 0.6 : 0);
    this.clouds = [];
    const n = this.photo ? 0 : Math.round(cc * 14);
    for (let i = 0; i < n; i++) {
      this.clouds.push({
        x: r() * w * 1.2 - w * 0.1,
        y: r() * this.horizon * 0.55,
        speed: (4 + r() * 10) * this.scale,
        sprite: cloudSprite((180 + r() * 260) * this.scale, cloudColor, 0.35 + 0.5 * lightness * (0.6 + r() * 0.4)),
      });
    }
    const area = (w * h) / (1920 * 1080);
    const drops = s.weather === "rain" ? 320 : s.weather === "storm" ? 520 : 0;
    this.drops = Array.from({ length: Math.round(drops * area) }, () => ({
      x: r() * w, y: r() * h, v: (700 + r() * 500) * this.scale, len: (10 + r() * 14) * this.scale,
    }));
    const flakes = s.weather === "snow" ? 260 : 0;
    this.flakes = Array.from({ length: Math.round(flakes * area) }, () => ({
      x: r() * w, y: r() * h, v: (25 + r() * 50) * this.scale, r: (1 + r() * 2.2) * this.scale, p: r() * 6.28,
    }));
    this.nextFlash = performance.now() + 3000 + r() * 6000;
  }

  private frame(t: number, dt: number) {
    const s = this.scene;
    const c = this.ctx;
    const { w, h } = this;
    if (!s) {
      c.fillStyle = "#050816";
      c.fillRect(0, 0, w, h);
      return;
    }
    c.drawImage(this.sky, 0, 0);

    for (const st of this.twinkle) {
      const a = 0.5 + 0.5 * Math.sin(t / 700 + st.p);
      c.fillStyle = `rgba(255,255,255,${a * 0.9})`;
      c.fillRect(st.x - 0.5, st.y - 0.5, 2 * this.scale, 2 * this.scale);
    }

    for (const cl of this.clouds) {
      cl.x += cl.speed * dt;
      if (cl.x > w) cl.x = -cl.sprite.width;
      c.drawImage(cl.sprite, cl.x, cl.y);
    }

    c.drawImage(this.city, 0, 0);

    if (s.weather === "fog") {
      for (let i = 0; i < 3; i++) {
        const y = this.horizon - h * 0.25 + i * h * 0.12;
        const off = ((t / 1000) * (6 + i * 3) * this.scale) % w;
        const g = c.createLinearGradient(0, y - h * 0.15, 0, y + h * 0.15);
        g.addColorStop(0, "rgba(210,214,222,0)");
        g.addColorStop(0.5, `rgba(210,214,222,${0.28 + 0.06 * Math.sin(t / 3000 + i)})`);
        g.addColorStop(1, "rgba(210,214,222,0)");
        c.fillStyle = g;
        c.save();
        c.translate(off, 0);
        c.fillRect(-w, y - h * 0.15, w * 2, h * 0.3);
        c.restore();
      }
      c.fillStyle = "rgba(200,205,215,0.18)";
      c.fillRect(0, 0, w, h);
    }

    if (this.drops.length) {
      c.strokeStyle = "rgba(190,205,230,0.45)";
      c.lineWidth = Math.max(1, this.scale);
      c.beginPath();
      for (const d of this.drops) {
        d.y += d.v * dt;
        d.x -= d.v * dt * 0.15;
        if (d.y > h) { d.y = -d.len; d.x = Math.random() * w * 1.1; }
        c.moveTo(d.x, d.y);
        c.lineTo(d.x - d.len * 0.15, d.y + d.len);
      }
      c.stroke();
    }

    if (this.flakes.length) {
      c.fillStyle = "rgba(255,255,255,0.85)";
      c.beginPath();
      for (const f of this.flakes) {
        f.y += f.v * dt;
        f.p += dt;
        const x = f.x + Math.sin(f.p) * 12 * this.scale;
        if (f.y > h) { f.y = -5; f.x = Math.random() * w; }
        c.moveTo(x + f.r, f.y);
        c.arc(x, f.y, f.r, 0, Math.PI * 2);
      }
      c.fill();
    }

    if (s.weather === "storm") {
      if (dt > 0 && t > this.nextFlash) {
        this.flash = 1;
        this.nextFlash = t + 4000 + Math.random() * 9000;
      }
      if (this.flash > 0.01) {
        c.fillStyle = `rgba(230,235,255,${this.flash * 0.55})`;
        c.fillRect(0, 0, w, h);
        this.flash *= Math.pow(0.002, dt);
      }
    }
  }
}

function cloudSprite(size: number, color: RGB, alpha: number) {
  const cv = document.createElement("canvas");
  cv.width = Math.ceil(size * 1.8);
  cv.height = Math.ceil(size * 0.8);
  const c = cv.getContext("2d")!;
  const r = Math.random;
  for (let i = 0; i < 9; i++) {
    const x = cv.width * (0.2 + r() * 0.6);
    const y = cv.height * (0.45 + r() * 0.2);
    const rad = size * (0.18 + r() * 0.2);
    const g = c.createRadialGradient(x, y, 0, x, y, rad);
    g.addColorStop(0, css(color, alpha * 0.6));
    g.addColorStop(1, css(color, 0));
    c.fillStyle = g;
    c.fillRect(0, 0, cv.width, cv.height);
  }
  return cv;
}

function moonSprite(rad: number, alpha: number) {
  const cv = document.createElement("canvas");
  cv.width = cv.height = Math.ceil(rad * 2);
  const c = cv.getContext("2d")!;
  c.fillStyle = `rgba(236,238,250,${alpha})`;
  c.beginPath();
  c.arc(rad, rad, rad, 0, Math.PI * 2);
  c.fill();
  c.globalCompositeOperation = "destination-out";
  c.beginPath();
  c.arc(rad * 1.45, rad * 0.8, rad * 0.9, 0, Math.PI * 2);
  c.fill();
  return cv;
}
