"""Собирает картинки сцен из реальных фото с GitHub.

Скачивает исходники, обрезает под 16:9, делает цветокоррекцию под время суток
и добавляет погоду: дождь с глубиной и подсветкой от огней, свечение огней,
дымку, туман, снег с размытием по глубине.
Результат: src-tauri/assets/scenes/{phase}_{weather}.jpg

Запуск: python scripts/build-photos.py [сцена ...]   (нужны pillow и numpy)
"""
import random
import sys
import urllib.request
import zlib
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

W, H = 2560, 1440
OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "assets" / "scenes"
CACHE = Path(__file__).resolve().parent / ".photo-cache"

WALLS = "https://raw.githubusercontent.com/dharmx/walls/main/"
WIZ = "https://raw.githubusercontent.com/Cyndakwil/Wallpaper-Wizard/master/assets/Random/"

SRC = {
    "tokyo_night": WALLS + "architecture/a_city_at_night_with_many_tall_buildings.jpg",
    "night_trails": WALLS + "architecture/a_city_skyline_at_night.jpg",
    "skyline_dusk": WALLS + "architecture/a_city_skyline_at_night_01.jpg",
    "canal_night": WALLS + "unsorted/a_narrow_canal_with_boats_and_buildings_at_night.jpg",
    "city_snow_night": WALLS + "architecture/a_city_in_the_snow.jpg",
    "fog_road_night": WALLS + "fogsmoke/a_car_on_a_road_with_fog.jpg",
    "nyc_day": WALLS + "architecture/a_city_with_many_tall_buildings.jpg",
    "tracks_fog": WALLS + "cold/a_train_tracks_in_a_foggy_day.jpg",
    "walk_snow": WALLS + "unsorted/a_person_walking_in_the_snow.jpg",
    "dark_mountains": WALLS + "mountain/a_mountain_range_with_dark_clouds.jpg",
    "city_by_water": WALLS + "architecture/a_city_by_the_water.jpg",
    "city_clouds": WALLS + "mountain/a_city_in_the_distance_with_clouds.jpg",
    "trees_fog": WALLS + "fogsmoke/a_group_of_trees_in_the_fog.jpg",
    "snow_blue": WALLS + "cold/a_snowy_landscape_with_trees_and_a_light_on_it.jpg",
    "empire_sunset": WALLS + "architecture/a_city_skyline_with_a_tall_building.jpg",
    "towers_golden": WALLS + "architecture/a_group_of_tall_buildings_in_a_city.jpg",
    "overcast": WIZ + "Overcast.jpg",
    "thunder": WIZ + "Thunderstorm.jpg",
}

# Цветокоррекция по времени суток: тон теней, тон светов, баланс белого.
GRADES = {
    "dawn": dict(shadows=(0.30, 0.34, 0.52), highlights=(1.0, 0.80, 0.78), wb=(1.02, 0.98, 1.02), sat=0.95),
    "sunset": dict(shadows=(0.32, 0.22, 0.38), highlights=(1.0, 0.72, 0.45), wb=(1.06, 0.98, 0.88), sat=1.0),
    "day_grey": dict(shadows=(0.30, 0.33, 0.38), highlights=(0.92, 0.95, 1.0), wb=(0.97, 1.0, 1.04), sat=0.7),
    "night_cool": dict(shadows=(0.10, 0.14, 0.24), highlights=(1.0, 0.92, 0.80), wb=(0.96, 1.0, 1.06), sat=0.9),
}

# Сцена: исходник и цепочка эффектов (имя, параметры).
SCENES = {
    "night_clear": ("tokyo_night", [("bloom", 0.35)]),
    "night_cloudy": ("night_trails", [("bloom", 0.3)]),
    "night_fog": ("fog_road_night", [("bloom", 0.5)]),
    "night_rain": ("canal_night", [("grade", "night_cool", 0.5), ("haze", 0.12), ("bloom", 0.8), ("rain", 1.0), ("vignette", 0.35)]),
    "night_snow": ("city_snow_night", [("bloom", 0.4)]),
    "night_storm": ("thunder", [("rain", 0.7), ("vignette", 0.3)]),
    "dawn_clear": ("city_by_water", []),
    "dawn_cloudy": ("city_clouds", [("grade", "dawn", 0.6)]),
    "dawn_fog": ("trees_fog", [("grade", "dawn", 0.5), ("fog", 0.25)]),
    "dawn_rain": ("city_clouds", [("exposure", 0.8), ("grade", "dawn", 0.5), ("grade", "day_grey", 0.4), ("haze", 0.3), ("rain", 0.9), ("vignette", 0.3)]),
    "dawn_snow": ("snow_blue", [("exposure", 1.15), ("grade", "dawn", 0.55), ("haze", 0.15), ("snow", 0.8)]),
    "dawn_storm": ("dark_mountains", [("grade", "dawn", 0.9), ("grade", "dawn", 0.4), ("storm", 0.9), ("rain", 1.4), ("vignette", 0.45)]),
    "day_clear": ("nyc_day", []),
    "day_cloudy": ("overcast", []),
    "day_fog": ("tracks_fog", []),
    "day_rain": ("nyc_day", [("exposure", 0.78), ("grade", "day_grey", 0.85), ("haze", 0.35), ("rain", 1.0), ("vignette", 0.25)]),
    "day_snow": ("walk_snow", [("snow", 0.4)]),
    "day_storm": ("overcast", [("storm", 1.0), ("rain", 1.4), ("vignette", 0.45)]),
    "sunset_clear": ("empire_sunset", []),
    "sunset_cloudy": ("skyline_dusk", [("bloom", 0.4)]),
    "sunset_fog": ("towers_golden", [("grade", "sunset", 0.35), ("fog", 0.35), ("bloom", 0.4)]),
    "sunset_rain": ("towers_golden", [("exposure", 0.75), ("grade", "sunset", 0.3), ("haze", 0.2), ("bloom", 0.7), ("rain", 1.0), ("vignette", 0.35)]),
    "sunset_snow": ("walk_snow", [("exposure", 0.92), ("grade", "sunset", 0.42), ("snow", 0.3), ("vignette", 0.3)]),
    "sunset_storm": ("dark_mountains", [("grade", "sunset", 0.3), ("storm", 0.8), ("rain", 1.3), ("vignette", 0.45)]),
}


# ---- Вспомогательное ----

def fetch(name):
    CACHE.mkdir(exist_ok=True)
    path = CACHE / name
    if not path.exists():
        print("скачиваю", SRC[name])
        with urllib.request.urlopen(SRC[name], timeout=120) as r:
            path.write_bytes(r.read())
    return Image.open(path).convert("RGB")


def cover(im):
    k = max(W / im.width, H / im.height)
    im = im.resize((round(im.width * k), round(im.height * k)), Image.LANCZOS)
    x, y = (im.width - W) // 2, (im.height - H) // 2
    return im.crop((x, y, x + W, y + H))


def to_np(im):
    return np.asarray(im, dtype=np.float32) / 255.0


def to_im(a):
    return Image.fromarray((np.clip(a, 0, 1) * 255 + 0.5).astype(np.uint8))


def _box(a, r, axis):
    """Скользящее среднее радиуса r вдоль оси, края — повтором крайних пикселей."""
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
    r = max(1, int(round((np.sqrt(12 * sigma * sigma / 3 + 1) - 1) / 2)))
    for _ in range(3):
        a = _box(_box(a, r, 0), r, 1)
    return a


def lum(a):
    return a[..., 0] * 0.2126 + a[..., 1] * 0.7152 + a[..., 2] * 0.0722


def ambient(a):
    """Средний цвет неба (верхняя треть) — им окрашиваются дымка и осадки."""
    return a[: H // 3].reshape(-1, 3).mean(axis=0)


def vgrad(center, width):
    """Вертикальная колоколообразная маска (H,1)."""
    y = np.linspace(0, 1, H, dtype=np.float32)[:, None]
    return np.exp(-(((y - center) / width) ** 2))


# ---- Эффекты ----

def fx_exposure(a, k):
    return a * k


def fx_grade(a, name, k):
    g = GRADES[name]
    l = lum(a)[..., None]
    sh, hi = np.array(g["shadows"]), np.array(g["highlights"])
    # Сплит-тонирование: тени тянутся к тону теней, света — к тону светов, яркость сохраняется.
    tone = sh * (1 - l) + hi * l
    tone = tone / np.maximum(lum(tone)[..., None], 1e-3)
    out = a * (1 - k) + (a * tone) * k
    out = out * np.array(g["wb"]) ** k
    gray = lum(out)[..., None]
    sat = 1 + (g["sat"] - 1) * k
    return gray + (out - gray) * sat


def fx_bloom(a, k):
    """Свечение ярких огней — на мокром воздухе оно сильнее."""
    l = lum(a)
    bright = a * np.clip((l - 0.55) / 0.45, 0, 1)[..., None]
    glow = blur(bright, 18) * 0.7 + blur(bright, 60) * 0.5
    return 1 - (1 - a) * (1 - glow * k)


def fx_haze(a, k):
    """Атмосферная дымка: сильнее у горизонта, цвет берётся с неба."""
    amb = ambient(a) * 0.7 + 0.3 * lum(a).mean()
    mask = (0.4 + 0.6 * vgrad(0.55, 0.35)) * k
    out = a * (1 - mask[..., None]) + amb * mask[..., None]
    # Поднятые тени: в дождь чёрного почти не бывает.
    return out * (1 - 0.06 * k) + 0.06 * k * amb


def fx_fog(a, k):
    amb = np.clip(ambient(a) * 0.5 + 0.55, 0, 1)
    soft = blur(a, 8)
    mask = np.clip(k * (0.55 + 0.9 * vgrad(0.62, 0.3)), 0, 0.9)[..., None]
    return (a * (1 - mask * 0.6) + soft * mask * 0.6) * (1 - mask) + amb * mask


def fx_storm(a, k):
    """Тяжёлое небо: затемнение сверху, холодный зелёно-серый тон, контраст."""
    y = np.linspace(0, 1, H, dtype=np.float32)[:, None, None]
    sky = 1 - k * 0.45 * np.clip(1 - y * 1.6, 0, 1)
    out = a * sky
    out = out * (1 - 0.25 * k) + 0.25 * k * out * np.array([0.85, 0.95, 1.0])
    gray = lum(out)[..., None]
    out = gray + (out - gray) * (1 - 0.35 * k)
    return np.clip((out - 0.5) * (1 + 0.2 * k) + 0.5 - 0.04 * k, 0, 1)


def streak_layer(rnd, count, length, width, angle, soft):
    """Слой капель: точки, размазанные по направлению падения (motion blur)."""
    pad = int(length * 1.5)
    hh, ww = H + 2 * pad, W + 2 * pad
    m = np.zeros((hh, ww), np.float32)
    ys = rnd.integers(0, hh, count)
    xs = rnd.integers(0, ww, count)
    m[ys, xs] = rnd.uniform(0.4, 1.0, count)
    if width > 1:
        m = _box(_box(m, width // 2, 0), width // 2, 1) * (width | 1) ** 2
    # Размазываем по вертикали скользящим средним, потом наклоняем.
    c = np.cumsum(m, axis=0)
    m = (c[length:] - c[:-length]) / length * length ** 0.5
    img = Image.fromarray(m, "F").rotate(angle, resample=Image.BILINEAR, expand=False)
    m = np.asarray(img)
    top = (m.shape[0] - H) // 2
    left = (m.shape[1] - W) // 2
    m = m[top : top + H, left : left + W]
    if soft:
        m = blur(m, soft)
    return m


def fx_rain(a, k):
    rnd = np.random.default_rng(zlib.crc32(str(a.shape).encode()) + int(k * 100))
    l_soft = blur(lum(a), 30)
    # Капли видны там, где их подсвечивает фон или фонари.
    light = 0.35 + 0.9 * np.clip(l_soft, 0, 1) ** 0.8
    amb = np.clip(ambient(a) * 0.4 + 0.75, 0, 1)
    layers = [
        # (кол-во, длина, толщина, мягкость, яркость) — от дальнего к ближнему
        (int(26000 * k), 18, 1, 0.8, 0.10),
        (int(9000 * k), 45, 1, 0.9, 0.16),
        (int(1600 * k), 110, 3, 2.2, 0.16),
        (int(160 * k), 260, 7, 7.0, 0.12),
    ]
    out = a
    for count, length, width, soft, alpha in layers:
        m = streak_layer(rnd, count, length, width, 10, soft)
        m = np.clip(m / max(m.max(), 1e-6) * 2.2, 0, 1) * alpha * light
        out = out + (amb - out) * m[..., None]
    # Мелкая водяная взвесь у земли.
    mist = vgrad(0.8, 0.25)[..., None] * 0.08 * k
    return out * (1 - mist) + amb * mist


def fx_snow(a, k):
    rnd = random.Random(zlib.crc32(b"snow") + int(k * 100))
    amb = np.clip(ambient(a) * 0.25 + 0.85, 0, 1)
    out = a
    # (кол-во, радиус, размытие, яркость): дальние мелкие и резкие, ближние крупные и размытые.
    for count, r, soft, alpha in [(9000, (0.8, 1.6), 0.6, 0.55), (1800, (2, 3.5), 1.4, 0.75),
                                  (220, (6, 11), 4.0, 0.55), (28, (22, 40), 14, 0.35)]:
        mask = Image.new("F", (W, H), 0)
        d = ImageDraw.Draw(mask)
        for _ in range(int(count * k)):
            x, y, rr = rnd.uniform(0, W), rnd.uniform(0, H), rnd.uniform(*r)
            d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=rnd.uniform(0.5, 1.0))
        m = blur(np.asarray(mask, dtype=np.float32), soft) * alpha
        out = out + (amb - out) * np.clip(m, 0, 1)[..., None]
    return out


def fx_vignette(a, k):
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    d = ((x / W - 0.5) ** 2 + (y / H - 0.5) ** 2 * 0.8) ** 0.5
    return a * (1 - k * np.clip(d - 0.25, 0, 1) ** 1.5 * 2)[..., None]


FX = {name[3:]: fn for name, fn in globals().items() if name.startswith("fx_")}


def build(key):
    src, chain = SCENES[key]
    a = to_np(cover(fetch(src)))
    for name, *args in chain:
        a = np.clip(FX[name](a, *args), 0, 1)
    OUT.mkdir(parents=True, exist_ok=True)
    # Лёгкое зерно, чтобы дорисованное не выглядело «пластиковым».
    grain = np.random.default_rng(zlib.crc32(key.encode())).normal(0, 0.008, (H, W, 1)).astype(np.float32)
    to_im(a + grain * bool(chain)).save(OUT / f"{key}.jpg", quality=88, optimize=True, progressive=True)
    print("готово", key)


if __name__ == "__main__":
    for k in sys.argv[1:] or SCENES:
        build(k)
