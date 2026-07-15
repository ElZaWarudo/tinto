import { describe, expect, it, vi } from "vitest";
import type { BusState } from "../bus/store";
import type { RepoDelta } from "../bus/contract";
import {
  collectRelevantNotifications,
  enableNotifications,
  type NotificationAdapter,
} from "./notifications";
import { qualityStore } from "./state";

const delta = (repo: string, over: Partial<RepoDelta> = {}): RepoDelta => ({
  repo,
  revision: 7,
  status: { modified: [], staged: [], untracked: [] },
  branch: null,
  head: null,
  last_activity_ms: 1_700_000_000_000,
  error: null,
  metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
  gitleaks_configured: false,
  agents_md_configured: false,
  secret_scan_status: { state: "not_run" },
  ...over,
});

describe("quality notifications", () => {
  it("collects high-attention events with redacted copy and stable keys", () => {
    const state: BusState = {
      repos: {
        "/secret/root": delta("/secret/root", {
          error: { class: "terminal", category: "repo-removed", message: "/secret/root vanished" },
          signals: [
            {
              kind: "possible_secret",
              severity: "critical",
              path: "private/.env",
              message: "token ABC123",
            },
          ],
        }),
      },
      fsEventsByRepo: {
        "/secret/root": [
          {
            path: "private/.env",
            kind: "modified",
            timestamp_ms: 1_700_000_000_000,
            size: 42,
            size_delta: 3,
            signals: [
              {
                kind: "sensitive_path",
                severity: "warning",
                path: "private/.env",
                message: "Sensitive watched file changed",
              },
            ],
          },
        ],
      },
      activity: {},
      diffs: {},
      config: null,
      configStatus: "ready",
      configError: null,
      snapshotStatus: "ready",
      snapshotError: null,
      loaded: true,
      watching: { available: false, reason: "inotify" },
    };

    const notifications = collectRelevantNotifications(state, () => "Secrets");

    expect(notifications).toHaveLength(4);
    expect(notifications.map((item) => item.key)).toEqual(
      expect.arrayContaining([
        "watching:degraded:inotify",
        "repo-error:/secret/root:7:repo-removed",
        "signal:/secret/root:7:possible_secret:private/.env",
        "fs-signal:/secret/root:1700000000000:modified:private/.env:sensitive_path",
      ]),
    );
    expect(notifications.map((item) => item.body)).toEqual(
      expect.arrayContaining([
        "Observación degradada. Los datos siguen disponibles bajo demanda.",
        "Se detectó una señal crítica: posible secreto.",
        "Se detectó una señal de archivo observado: advertencia.",
      ]),
    );
    for (const notification of notifications) {
      expect(notification.title).not.toContain("/secret/root");
      expect(notification.body).not.toContain("/secret/root");
      expect(notification.body).not.toContain("private/.env");
      expect(notification.body).not.toContain("ABC123");
    }
  });

  it("records permission outcomes in the quality store", async () => {
    const adapter: NotificationAdapter = {
      ensurePermission: vi.fn().mockResolvedValue("denied"),
      send: vi.fn(),
    };

    await expect(enableNotifications(adapter)).resolves.toBe(false);
    expect(qualityStore.getState().notificationsEnabled).toBe(false);
    expect(qualityStore.getState().notificationStatus).toBe("denied");

    adapter.ensurePermission = vi.fn().mockResolvedValue("granted");
    await expect(enableNotifications(adapter)).resolves.toBe(true);
    expect(qualityStore.getState().notificationsEnabled).toBe(true);
    expect(qualityStore.getState().notificationStatus).toBe("ready");
  });
});
