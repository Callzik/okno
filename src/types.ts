export type Phase = "night" | "dawn" | "day" | "sunset";
export type WeatherKind = "clear" | "cloudy" | "fog" | "rain" | "snow" | "storm";
export type Mode = "static" | "live";

export interface City {
  name: string;
  country?: string | null;
  admin1?: string | null;
  latitude: number;
  longitude: number;
  timezone?: string | null;
}

/** Событие scene-update из Rust. */
export interface SceneUpdate {
  phase: Phase;
  weather: WeatherKind;
  sun_elevation: number;
  cloud_cover: number;
  temperature: number | null;
  image_path?: string | null;
}

export interface SceneState {
  city: City | null;
  scene: string | null;
  phase: Phase | null;
  weather: WeatherKind | null;
  sun_elevation: number | null;
  cloud_cover: number | null;
  temperature: number | null;
  sunrise: string | null;
  sunset: string | null;
  updated_at: string | null;
  mode: Mode;
  custom_folder: string | null;
  preview: string | null;
  paused: boolean;
  pause_reason: string | null;
  offline: boolean;
  warning: string | null;
  image_path: string | null;
}

export const PHASES: Phase[] = ["night", "dawn", "day", "sunset"];
export const WEATHERS: WeatherKind[] = ["clear", "cloudy", "fog", "rain", "snow", "storm"];

export const PHASE_RU: Record<Phase, string> = {
  night: "Ночь",
  dawn: "Рассвет",
  day: "День",
  sunset: "Закат",
};

export const WEATHER_RU: Record<WeatherKind, string> = {
  clear: "ясно",
  cloudy: "облачно",
  fog: "туман",
  rain: "дождь",
  snow: "снег",
  storm: "гроза",
};

export function toUpdate(s: SceneState): SceneUpdate | null {
  if (!s.phase || !s.weather || s.sun_elevation == null) return null;
  return {
    phase: s.phase,
    weather: s.weather,
    sun_elevation: s.sun_elevation,
    cloud_cover: s.cloud_cover ?? 0,
    temperature: s.temperature,
    image_path: s.image_path,
  };
}
