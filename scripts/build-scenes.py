"""Собирает 24 картинки сцен из одной серии реальных фото.

Основа — динамические обои macOS Mojave: один и тот же вид на дюны, снятый
в разное время суток (5120×2880). Берём кадры ночи, рассвета, дня и заката,
а погоду добавляем поверх: облачность, туман, дождь, снег, грозу.
Результат: src-tauri/assets/scenes/{phase}_{weather}.jpg (2560×1440)

Запуск: python scripts/build-scenes.py [сцена ...]   (нужны pillow и numpy)
"""
import sys
import urllib.request
import zlib
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

W, H = 2560, 1440
ROOT = Path(__file__).resolve().parent
OUT = ROOT.parent / "src-tauri" / "assets" / "scenes"
CACHE = ROOT / ".photo-cache"
BASE_URL = "https://raw.githubusercontent.com/saint-13/Linux_Dynamic_Wallpapers/main/Dynamic_Wallpapers/Mojave/mojave_dynamic_{}.jpeg"

# Кадр серии для каждой фазы.
FRAMES = {"night": 16, "dawn": 2, "day": 8, "sunset": 12}
# Кадр с самым чётким контрастом неба и земли — по нему строится маска неба.
MASK_FRAME = 2

PHASES = ["night", "dawn", "day", "sunset"]
WEATHERS = ["clear", "cloudy", "fog", "rain", "snow", "storm"]


# ---- Загрузка и базовые операции ----

def load(frame):
    CACHE.mkdir(exist_ok=True)
    path = CACHE / f"mojave_{frame}.jpeg"
    if not path.exists():
        print("скачиваю кадр", frame)
        with urllib.request.urlopen(BASE_URL.format(frame), timeout=300) as r:
            path.write_bytes(r.read())
    im = Image.open(path).convert("RGB").resize((W, H), Image.LANCZOS)
    return np.asarray(im, dtype=np.float32) / 255.0


def save(a, name):
    OUT.mkdir(parents=True, exist_ok=True)
    grain = np.random.default_rng(zlib.crc32(name.encode())).normal(0, 0.007, (H, W, 1)).astype(np.float32)
    im = Image.fromarray((np.clip(a + grain, 0, 1) * 255 + 0.5).astype(np.uint8))
    im.save(OUT / f"{name}.jpg", quality=88, optimize=True, progressive=True)


def _box(a, r, axis):
    if r < 1:
        return a
    pad = [(0, 0)] * a.ndim
    pad[axis] = (r + 1, r)
    c = np.cumsum(np.pad(a, pad, mode="edge"), axis=axis, dtype=np.float64)
    n = a.shape[axis]
    hi = np.take(c, np.arange(2 * r + 1, 2 * r + 1 + n), axis=axis)
    lo = np.take(c, np.arange(0, n), axis=axis)
    return ((hi - lo) / (2 * r + 1)).astype(np.float32)


def blur(a, sigma):
    """Приближённое гауссово размытие тремя проходами box-фильтра."""
    if sigma <= 0.3:
        return a
    r = max(1, int(round((np.sqrt(4 * sigma * sigma + 1) - 1) / 2)))
    for _ in range(3):
        a = _box(_box(a, r, 0), r, 1)
    return a


def lum(a):
    return a[..., 0] * 0.2126 + a[..., 1] * 0.7152 + a[..., 2] * 0.0722


def desat(c, k):
    g = lum(c)[..., None] if c.ndim > 1 else lum(c)
    return g + (c - g) * (1 - k)


def mix(a, b, m):
    if np.ndim(m) == 2:
        m = m[..., None]
    return a + (b - a) * m


# ---- Маска неба и глубина ----

def sky_line():
    """Линия горизонта по столбцам: на кадре рассвета небо ярче земли."""
    l = lum(load(MASK_FRAME))
    ground = l < 0.45
    line = np.zeros(W, int)
    for x in range(W):
        run = np.convolve(ground[:, x].astype(int), np.ones(4, int), "valid")
        line[x] = np.argmax(run >= 4)
    pad = np.pad(line, 3, mode="edge")
    line = np.array([np.median(pad[i : i + 7]) for i in range(W)]).astype(int)
    return line - 2


LINE = None
SKY = None  # 1 — небо, 0 — земля, мягкий край
DIST = None  # 1 у горизонта (далеко), 0 у нижнего края (близко)


def init_masks():
    global LINE, SKY, DIST
    LINE = sky_line()
    y = np.arange(H, dtype=np.float32)[:, None]
    SKY = blur(np.clip((LINE[None, :] - y) / 3 + 0.5, 0, 1).astype(np.float32), 1.2)
    below = np.clip((y - LINE[None, :]) / (H - LINE.min()), 0, 1)
    DIST = (1 - below) ** 2.2 * (1 - SKY)


def sky_color(a):
    """Средний цвет неба у горизонта и вверху."""
    m = SKY > 0.9
    y = np.arange(H)[:, None] * np.ones((1, W))
    top = a[m & (y < 300)].mean(axis=0)
    low = a[m & (y > LINE.min() - 200)].mean(axis=0)
    return top, low


# ---- Облака ----

# Небо берём с реальных фото: пасмурное (монохромное, легко перекрасить) и грозовое.
SKIES = {
    "overcast": ("https://raw.githubusercontent.com/Cyndakwil/Wallpaper-Wizard/master/assets/Random/Overcast.jpg", 0.0, 0.46),
    "thunder": ("https://raw.githubusercontent.com/Cyndakwil/Wallpaper-Wizard/master/assets/Random/Thunderstorm.jpg", 0.0, 0.84),
}


def sky_photo(name):
    """Верхняя часть фото неба, растянутая на область неба сцены."""
    url, y0, y1 = SKIES[name]
    path = CACHE / f"sky_{name}.jpg"
    if not path.exists():
        print("скачиваю небо", name)
        with urllib.request.urlopen(url, timeout=300) as r:
            path.write_bytes(r.read())
    im = Image.open(path).convert("RGB")
    im = im.crop((0, int(im.height * y0), im.width, int(im.height * y1)))
    hs = int(LINE.max()) + 40
    k = max(W / im.width, hs / im.height)
    im = im.resize((round(im.width * k), round(im.height * k)), Image.LANCZOS)
    x = (im.width - W) // 2
    im = im.crop((x, im.height - hs, x + W, im.height))
    a = np.asarray(im, dtype=np.float32) / 255.0
    out = np.empty((H, W, 3), np.float32)
    out[:hs] = a
    out[hs:] = a[-1]
    return out


def clouds(a, phase, cover, dark, sky="overcast"):
    """Реальное пасмурное небо, перекрашенное под время суток.
    cover — насколько закрыто исходное небо, dark — насколько тяжёлые облака."""
    top, low = sky_color(a)
    tex = sky_photo(sky)
    t = lum(tex)
    lo, hi = np.percentile(t[: int(LINE.min())], [2, 98])
    t = np.clip((t - lo) / (hi - lo), 0, 1)
    base = desat((top + low) / 2, 0.6)
    light = np.clip(base * (1.25 - 0.6 * dark) + 0.06, 0, 1)
    if phase in ("dawn", "sunset"):
        light = mix(light, low, 0.55 * (1 - dark))
    shadow = desat(base, 0.4) * (0.5 - 0.32 * dark)
    col = shadow + (light - shadow) * t[..., None] ** (1 + 0.6 * dark)
    # При частичной облачности в светлых просветах видно исходное небо.
    alpha = np.clip(cover + (1 - cover) * (1 - t) * 1.6, 0, 1)
    return mix(a, mix(a, col, alpha), SKY)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def storm_sky(a, phase):
    """Грозовое небо с молниями. Ночью — как есть, на закате — перекрашено в тёплые тона."""
    tex = sky_photo("thunder")
    if phase == "sunset":
        t = lum(tex)
        warm = mix(np.array([0.10, 0.07, 0.10]), np.array([0.85, 0.48, 0.30]), np.clip(t * 1.6, 0, 1) ** 1.3)
        bolt = smoothstep(0.62, 0.9, t)
        tex = mix(warm, np.array([1.0, 0.95, 0.9]), bolt)
    return mix(a, tex, SKY)


# ---- Свет и погода на земле ----

def overcast_ground(a, k, darken):
    """Рассеянный свет: тени мягче, цвета глуше, всё темнее."""
    g = a * (1 - darken * k)
    # Без солнца пропадает перепад «освещённый склон / тень», мелкая фактура остаётся.
    l0 = lum(g)
    big = blur(l0, 60) + 0.02
    target = big * (1 - 0.6 * k) + np.median(l0[SKY < 0.1]) * 0.6 * k
    g = g * (target / big)[..., None]
    l = lum(g)[..., None]
    g = l + (g - l) * (1 - 0.35 * k)
    g = g ** (1 - 0.15 * k)  # приподнять тени
    return mix(a, g, 1 - SKY)


def aerial(a, color, k):
    """Воздушная перспектива: дальнее тонет в дымке."""
    return mix(a, color, np.clip(DIST * k, 0, 1))


def snow_ground(a, k):
    l = lum(a)
    ln = np.clip(l / max(np.percentile(l[SKY < 0.1], 99), 1e-3), 0, 1)
    white = np.array([0.93, 0.95, 1.0])
    # Тени на снегу голубые, освещённые места почти белые.
    col = mix(np.array([0.62, 0.68, 0.80]), white, ln ** 0.6)
    col = col * (0.55 + 0.45 * ln[..., None])
    exposure = np.clip(np.median(l[SKY < 0.1]) / 0.35, 0.15, 1.2)
    col = col * exposure
    return mix(a, col, (1 - SKY) * k)


def wet_ground(a, k):
    g = a * (1 - 0.18 * k)
    l = lum(g)[..., None]
    g = l + (g - l) * (1 + 0.15 * k)
    return mix(a, g, 1 - SKY)


def fog(a, color, k):
    y = np.arange(H, dtype=np.float32)[:, None]
    near_line = np.exp(-((y - LINE[None, :]) / 260.0) ** 2)
    m = np.clip(k * (0.35 + 0.65 * np.maximum(DIST, near_line) + 0.5 * SKY), 0, 0.97)
    soft = blur(a, 6)
    return mix(mix(a, soft, m * 0.7), color, m)


def streak_layer(rng, count, length, width, angle, soft):
    pad = int(length * 1.5)
    hh, ww = H + 2 * pad, W + 2 * pad
    m = np.zeros((hh, ww), np.float32)
    m[rng.integers(0, hh, count), rng.integers(0, ww, count)] = rng.uniform(0.4, 1.0, count)
    if width > 1:
        m = _box(_box(m, width // 2, 0), width // 2, 1) * (width | 1) ** 2
    c = np.cumsum(m, axis=0)
    m = (c[length:] - c[:-length]) / length * length ** 0.5
    m = np.asarray(Image.fromarray(m, "F").rotate(angle, resample=Image.BILINEAR))
    t, l = (m.shape[0] - H) // 2, (m.shape[1] - W) // 2
    m = m[t : t + H, l : l + W]
    return blur(m, soft) if soft else m


def rain(a, k, seed):
    rng = np.random.default_rng(seed)
    light = 0.3 + 0.9 * np.clip(blur(lum(a), 30), 0, 1) ** 0.8
    top, low = sky_color(a)
    col = np.clip(desat(low, 0.6) * 0.5 + 0.55, 0, 1)
    for count, length, width, soft, alpha in [
        (int(26000 * k), 18, 1, 0.8, 0.10),
        (int(9000 * k), 45, 1, 0.9, 0.16),
        (int(1600 * k), 110, 3, 2.2, 0.16),
        (int(160 * k), 260, 7, 7.0, 0.12),
    ]:
        m = streak_layer(rng, count, length, width, 10, soft)
        m = np.clip(m / max(m.max(), 1e-6) * 2.2, 0, 1) * alpha * light
        a = mix(a, col, m)
    return a


def snowfall(a, k, seed):
    rng = np.random.default_rng(seed)
    top, low = sky_color(a)
    col = np.clip(desat(low, 0.7) * 0.25 + 0.82, 0, 1) * (0.55 + 0.45 * min(1.0, lum(low) * 2.5))
    for count, r, soft, alpha in [(9000, (0.8, 1.6), 0.6, 0.55), (1800, (2, 3.5), 1.4, 0.75),
                                  (220, (6, 11), 4.0, 0.5), (28, (22, 40), 14, 0.3)]:
        mask = Image.new("F", (W, H), 0)
        d = ImageDraw.Draw(mask)
        for _ in range(int(count * k)):
            x, y, rr = rng.uniform(0, W), rng.uniform(0, H), rng.uniform(*r)
            d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=float(rng.uniform(0.5, 1.0)))
        m = blur(np.asarray(mask, dtype=np.float32), soft) * alpha
        a = mix(a, col, np.clip(m, 0, 1))
    return a


def stars(a, k, seed):
    rng = np.random.default_rng(seed)
    m = np.zeros((H, W), np.float32)
    n = 2600
    ys, xs = rng.integers(0, H, n), rng.integers(0, W, n)
    m[ys, xs] = rng.uniform(0.2, 1.0, n) ** 3
    m = blur(m, 0.7) * 6
    y = np.arange(H, dtype=np.float32)[:, None]
    fade = np.clip((LINE[None, :] - 120 - y) / 400, 0, 1)
    return mix(a, np.array([0.95, 0.96, 1.0]), np.clip(m * fade * SKY * k, 0, 1))


def vignette(a, k):
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    d = ((xx / W - 0.5) ** 2 + (yy / H - 0.5) ** 2 * 0.8) ** 0.5
    return a * (1 - k * np.clip(d - 0.25, 0, 1) ** 1.5 * 2)[..., None]


# ---- Сцены ----

def build(phase, weather):
    a = load(FRAMES[phase])
    seed = zlib.crc32(f"{phase}_{weather}".encode())
    night = phase == "night"
    top, low = sky_color(a)

    if weather == "clear":
        if night:
            a = stars(a, 1.0, seed)
    elif weather == "cloudy":
        a = clouds(a, phase, cover=0.6, dark=0.1)
        a = overcast_ground(a, 0.7, 0.12)
        a = aerial(a, desat(low, 0.5) * 0.9, 0.25)
    elif weather == "fog":
        a = clouds(a, phase, cover=1.0, dark=0.0)
        a = overcast_ground(a, 0.8, 0.05)
        color = np.clip(desat(low, 0.6) * (0.55 if night else 1.0) + (0.08 if night else 0.12), 0, 1)
        a = fog(a, color, 0.75)
    elif weather == "rain":
        a = clouds(a, phase, cover=1.0, dark=0.5)
        a = overcast_ground(a, 1.0, 0.25)
        a = wet_ground(a, 1.0)
        a = aerial(a, desat(low, 0.6) * 0.6, 0.55)
        a = rain(a, 1.0, seed)
        a = vignette(a, 0.25)
    elif weather == "snow":
        a = clouds(a, phase, cover=1.0, dark=0.15)
        a = snow_ground(a, 0.85)
        a = overcast_ground(a, 0.6, 0.0)
        a = aerial(a, np.clip(desat(low, 0.6) * 0.8 + 0.12, 0, 1) * (0.5 if night else 1), 0.5)
        a = snowfall(a, 0.8, seed)
    elif weather == "storm":
        if phase in ("night", "sunset"):
            a = storm_sky(a, phase)
        else:
            a = clouds(a, phase, cover=1.0, dark=0.85)
        a = overcast_ground(a, 1.0, 0.3 if night else 0.42)
        a = wet_ground(a, 1.0)
        a = aerial(a, desat(low, 0.6) * 0.35, 0.6)
        a = rain(a, 1.4, seed)
        a = vignette(a, 0.4)
    save(np.clip(a, 0, 1), f"{phase}_{weather}")
    print("готово", f"{phase}_{weather}")


if __name__ == "__main__":
    init_masks()
    keys = sys.argv[1:] or [f"{p}_{w}" for p in PHASES for w in WEATHERS]
    for key in keys:
        build(*key.split("_"))
