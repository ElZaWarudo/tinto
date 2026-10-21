// Whole-app colour theme. The resolved theme is set as `data-theme` on the root
// element, where App.css redefines its colour tokens. Persisted to
// localStorage; every window (main and detached) hydrates it on start and
// follows changes made in another window through the `storage` event.
// "system" picks Oscuro or Claro from the OS colour scheme and follows it.

import { useSyncExternalStore } from "react";

export type Theme = "dark" | "purple" | "plum" | "light";
export type ThemeSetting = Theme | "system";

export const THEME_OPTIONS: { value: ThemeSetting; label: string }[] = [
  { value: "dark", label: "Oscuro" },
  { value: "purple", label: "Morado oscuro" },
  { value: "plum", label: "Ciruela" },
  { value: "light", label: "Claro" },
  { value: "system", label: "Según el sistema" },
];

export const THEME_STORAGE_KEY = "tinto:theme";
const DEFAULT_SETTING: ThemeSetting = "dark";
const LIGHT_QUERY = "(prefers-color-scheme: light)";

function isThemeSetting(value: unknown): value is ThemeSetting {
  return THEME_OPTIONS.some((option) => option.value === value);
}

function readPersisted(): ThemeSetting {
  try {
    const raw = localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeSetting(raw) ? raw : DEFAULT_SETTING;
  } catch {
    return DEFAULT_SETTING;
  }
}

function lightQuery(): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(LIGHT_QUERY)
    : null;
}

class ThemeStore {
  private setting: ThemeSetting = DEFAULT_SETTING;
  private listeners = new Set<() => void>();
  private hydrated = false;

  getSetting = (): ThemeSetting => this.setting;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  /** The theme in effect: the setting, or the OS scheme for "system". */
  resolved(): Theme {
    if (this.setting !== "system") return this.setting;
    return lightQuery()?.matches ? "light" : "dark";
  }

  /** Apply the persisted theme and follow the OS and other windows. Call once
   * per window, before the first render. */
  hydrate() {
    if (this.hydrated) return;
    this.hydrated = true;
    this.apply(readPersisted());
    lightQuery()?.addEventListener("change", () => {
      if (this.setting === "system") this.apply("system");
    });
    window.addEventListener("storage", (event) => {
      if (event.key === THEME_STORAGE_KEY) this.apply(readPersisted());
    });
  }

  set(setting: ThemeSetting) {
    try {
      localStorage.setItem(THEME_STORAGE_KEY, setting);
    } catch {
      /* private mode / unavailable storage — keep the in-memory theme */
    }
    this.apply(setting);
  }

  private apply(setting: ThemeSetting) {
    this.setting = setting;
    document.documentElement.dataset.theme = this.resolved();
    this.listeners.forEach((l) => l());
  }
}

export const themeStore = new ThemeStore();

export function useThemeSetting(): ThemeSetting {
  return useSyncExternalStore(themeStore.subscribe, themeStore.getSetting);
}

/** The theme in effect, re-rendering when it changes (including OS changes). */
export function useResolvedTheme(): Theme {
  return useSyncExternalStore(themeStore.subscribe, () => themeStore.resolved());
}
