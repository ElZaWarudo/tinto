import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup, configure } from "@testing-library/react";
import { qualityStore } from "./qol/state";

// A file's first render of a whole panel (React, dockview and jsdom warming
// up) takes about a second on an idle machine, so Testing Library's default
// 1 s wait failed it whenever the machine was busy. `findBy`/`waitFor` return
// as soon as the element shows, so a longer limit only delays real failures.
configure({ asyncUtilTimeout: 5000 });

// jsdom lacks ResizeObserver / matchMedia, which dockview and themed UI touch.
if (!("ResizeObserver" in globalThis)) {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;
}
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}

afterEach(() => {
  qualityStore.reset();
  cleanup();
});
