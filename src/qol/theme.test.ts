import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// A controllable "(prefers-color-scheme: light)" media query.
function mockSystemScheme(light: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    matches: light,
    media: "(prefers-color-scheme: light)",
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
  };
  vi.spyOn(window, "matchMedia").mockImplementation(() => query as unknown as MediaQueryList);
  return {
    set(next: boolean) {
      query.matches = next;
      listeners.forEach((listener) => listener());
    },
  };
}

// The store hydrates once per window, so each test loads a fresh module.
async function freshStore() {
  vi.resetModules();
  return import("./theme");
}

const rootTheme = () => document.documentElement.dataset.theme;

describe("theme", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("starts in Oscuro when nothing is saved", async () => {
    mockSystemScheme(true);
    const { themeStore } = await freshStore();
    themeStore.hydrate();
    expect(themeStore.getSetting()).toBe("dark");
    expect(rootTheme()).toBe("dark");
  });

  it("applies data-theme immediately and persists the setting", async () => {
    mockSystemScheme(false);
    const { themeStore, THEME_STORAGE_KEY } = await freshStore();
    themeStore.hydrate();

    themeStore.set("purple");
    expect(rootTheme()).toBe("purple");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("purple");

    themeStore.set("plum");
    expect(rootTheme()).toBe("plum");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("plum");

    themeStore.set("light");
    expect(rootTheme()).toBe("light");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
  });

  it("restores the saved setting on start and ignores unknown values", async () => {
    mockSystemScheme(false);
    localStorage.setItem("tinto:theme", "purple");
    let { themeStore } = await freshStore();
    themeStore.hydrate();
    expect(rootTheme()).toBe("purple");

    localStorage.setItem("tinto:theme", "neon");
    ({ themeStore } = await freshStore());
    themeStore.hydrate();
    expect(themeStore.getSetting()).toBe("dark");
    expect(rootTheme()).toBe("dark");
  });

  it("follows the OS colour scheme for Según el sistema", async () => {
    const system = mockSystemScheme(true);
    const { themeStore } = await freshStore();
    themeStore.hydrate();

    themeStore.set("system");
    expect(rootTheme()).toBe("light");

    system.set(false);
    expect(rootTheme()).toBe("dark");

    system.set(true);
    expect(rootTheme()).toBe("light");

    // An explicit theme no longer follows the OS.
    themeStore.set("purple");
    system.set(false);
    expect(rootTheme()).toBe("purple");
  });

  it("turns transitions off for the frame a theme change lands in", async () => {
    mockSystemScheme(false);
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    });
    const { themeStore } = await freshStore();
    themeStore.hydrate();
    frames.splice(0).forEach((frame) => frame(0));

    themeStore.set("light");
    expect(document.documentElement.dataset.themeSwitching).toBe("");
    frames.splice(0).forEach((frame) => frame(0));
    expect(document.documentElement.dataset.themeSwitching).toBeUndefined();

    // Re-applying the same theme leaves transitions alone.
    themeStore.set("light");
    expect(document.documentElement.dataset.themeSwitching).toBeUndefined();
    vi.unstubAllGlobals();
  });

  it("follows a change saved by another window", async () => {
    mockSystemScheme(false);
    const { themeStore, THEME_STORAGE_KEY } = await freshStore();
    themeStore.hydrate();

    localStorage.setItem(THEME_STORAGE_KEY, "light");
    window.dispatchEvent(new StorageEvent("storage", { key: THEME_STORAGE_KEY }));
    expect(themeStore.getSetting()).toBe("light");
    expect(rootTheme()).toBe("light");
  });
});
