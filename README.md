# Окно

Обои Windows 10/11 по реальному положению солнца и погоде в вашем городе. Два режима: статичный (картинки) и живой (анимированная сцена под иконками).

## Сборка на Windows

Нужны: [Node.js 20+](https://nodejs.org), [Rust](https://rustup.rs) (MSVC), Visual Studio Build Tools с компонентом «Разработка классических приложений на C++». WebView2 уже есть в Windows 10/11.

```
npm install
npm run tauri dev      # запуск для разработки
npm run tauri build    # установщик: src-tauri/target/release/bundle/nsis/Okno_0.1.0_x64-setup.exe
```

Проверка Rust: `cd src-tauri && cargo clippy -- -D warnings && cargo test`.

## Картинки сцен

`src-tauri/assets/scenes/{phase}_{weather}.jpg`, где phase — `night`, `dawn`, `day`, `sunset`, weather — `clear`, `cloudy`, `fog`, `rain`, `snow`, `storm`. Если картинки нет, берётся запасная: `sunset_storm → sunset_rain → sunset_cloudy → sunset_clear`.

Встроенные картинки — одна и та же реалистичная сцена (дюны, кадры динамических обоев macOS Mojave) для всех 24 вариантов: меняется только время суток и погода. Собираются скриптом `python scripts/build-scenes.py` (нужны pillow и numpy): берёт кадры ночь/рассвет/день/закат, заменяет небо реальными фото облачности и грозы с перекраской под фазу, добавляет мокрую землю и дождь с глубиной, снег, туман, дымку. `npm run scenes` генерирует заглушки-градиенты для отсутствующих файлов.

Своя папка выбирается в настройках, имена файлов те же (jpg, jpeg, png, webp). Чего нет в своей папке, берётся из встроенных.

## Устройство

- `src-tauri/src/sun.rs` — высота солнца, восход/закат, фаза.
- `weather.rs` — Open-Meteo, коды WMO, поиск города, город по IP (ipwho.is, затем ipapi.co), кэш `weather_cache.json`.
- `scene.rs` — сцена и запасные варианты.
- `engine.rs` — состояние, цикл раз в минуту (погода раз в 20 минут, при ошибке повтор через 2), смена обоев только при смене картинки, возврат исходных обоев.
- `wallpaper/static_mode.rs` — IDesktopWallpaper на каждый монитор. Исходные обои сохраняются в `original_wallpaper.json` и возвращаются при выходе, в том числе после аварийного завершения.
- `wallpaper/live_mode.rs` — окно сцены на весь виртуальный рабочий стол, родитель — WorkerW (классический вариант и вариант 24H2, где WorkerW внутри Progman). Если не вышло, включается статичный режим с предупреждением. После перезапуска Explorer окно пересоздаётся в течение минуты.
- `wallpaper/pause.rs` — пауза (обои замирают) при развёрнутом окне, окне на весь монитор, полноэкранной игре (SHQueryUserNotificationState) и работе от батареи. Проверка раз в секунду.
- `src/live/renderer.ts` — живая сцена на Canvas 2D, 30 FPS: фото сцены как фон, поверх анимированные дождь, снег, туман и вспышки молний.
- `src/settings/` — окно настроек.

Данные приложения: `%APPDATA%\app.okno.wallpaper\`.
