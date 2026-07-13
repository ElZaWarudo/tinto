import { browser, expect } from "@wdio/globals";
import { resolve } from "node:path";

interface NativeShellResult {
  error?: string;
  ping?: {
    message: string;
    timestamp_ms: number;
  };
  firstRunSeen?: boolean;
  dashboardSeen?: boolean;
  bodyText?: string;
  recentBefore?: string | null;
  recentAfter?: string | null;
}

describe("Tinto native shell", () => {
  it("loads through real Tauri IPC with isolated state", async () => {
    const result = await browser.executeAsync<NativeShellResult>((done) => {
      const run = async (): Promise<NativeShellResult> => {
        const waitFor = async <T extends Element>(
          selector: string,
          timeoutMs: number,
        ): Promise<T> => {
          const deadline = Date.now() + timeoutMs;

          while (Date.now() < deadline) {
            const element = document.querySelector<T>(selector);
            if (element) return element;
            await new Promise((resolve) => setTimeout(resolve, 50));
          }

          throw new Error(`Timed out waiting for ${selector}`);
        };

        const tauri = (
          window as typeof window & {
            __TAURI__?: {
              core: {
                invoke<T>(command: string): Promise<T>;
              };
            };
          }
        ).__TAURI__;

        if (!tauri) {
          throw new Error("The E2E Tauri API is unavailable");
        }

        const recentKey = "tinto:recent-workbenches:v1";
        const recentBefore = localStorage.getItem(recentKey);
        const ping = await tauri.core.invoke<{ message: string; timestamp_ms: number }>("ping");
        await waitFor<HTMLElement>('[data-testid="first-run"]', 15_000);

        const input = await waitFor<HTMLInputElement>('[data-testid="wb-name"]', 5_000);
        const setInputValue = Object.getOwnPropertyDescriptor(
          HTMLInputElement.prototype,
          "value",
        )?.set;
        if (!setInputValue) {
          throw new Error("The native input value setter is unavailable");
        }
        setInputValue.call(input, "E2E aislado");
        input.dispatchEvent(new Event("input", { bubbles: true }));
        input.dispatchEvent(new Event("change", { bubbles: true }));

        const createButton = await waitFor<HTMLButtonElement>('[data-testid="create-wb"]', 5_000);
        const enableDeadline = Date.now() + 5_000;
        while (createButton.disabled && Date.now() < enableDeadline) {
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
        if (createButton.disabled) {
          throw new Error("The create-workbench button remained disabled");
        }
        createButton.click();

        await waitFor<HTMLElement>('[data-testid="dashboard-add-repo"]', 30_000);

        const recentAfterCreate = localStorage.getItem(recentKey);
        if (recentAfterCreate) {
          try {
            const parsed = JSON.parse(recentAfterCreate) as unknown;
            if (Array.isArray(parsed)) {
              const next = parsed.filter((name) => name !== "E2E aislado");
              if (next.length > 0) localStorage.setItem(recentKey, JSON.stringify(next));
              else localStorage.removeItem(recentKey);
            }
          } catch {
            // Keep malformed user data untouched; the assertion below reports
            // if it still contains the E2E-only workbench name.
          }
        }

        return {
          bodyText: document.body.innerText,
          dashboardSeen: true,
          firstRunSeen: true,
          ping,
          recentAfter: localStorage.getItem(recentKey),
          recentBefore,
        };
      };

      void run()
        .then(done)
        .catch((error: unknown) => done({ error: String(error) }));
    });

    expect(result.error).toBeUndefined();
    expect(result.ping?.message).toBe("pong desde el backend de Tinto");
    expect(result.ping?.timestamp_ms).toBeGreaterThan(0);
    expect(result.firstRunSeen).toBe(true);
    expect(result.dashboardSeen).toBe(true);
    expect(result.bodyText).not.toContain("Tinto no pudo conectarse");
    expect(result.recentAfter ?? "").not.toContain("E2E aislado");

    const artifactDir = resolve(process.env.TINTO_E2E_ARTIFACT_DIR ?? "artifacts/tauri-e2e");
    await browser.saveScreenshot(resolve(artifactDir, "native-shell.png"));
  });
});
