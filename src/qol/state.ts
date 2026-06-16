import { useSyncExternalStore } from "react";

export type TimeWindow = "all" | "15m" | "1h" | "today";

export interface QualityFilters {
  search: string;
  repo: string;
  extension: string;
  timeWindow: TimeWindow;
}

export type NotificationStatus = "idle" | "checking" | "ready" | "denied" | "unavailable";

export interface QualityState {
  filters: QualityFilters;
  glanceMode: boolean;
  notificationsEnabled: boolean;
  notificationStatus: NotificationStatus;
  notificationMessage: string | null;
}

export const ALL_REPOS = "all";

export const DEFAULT_FILTERS: QualityFilters = {
  search: "",
  repo: ALL_REPOS,
  extension: "",
  timeWindow: "all",
};

const DEFAULT_STATE: QualityState = {
  filters: DEFAULT_FILTERS,
  glanceMode: false,
  notificationsEnabled: false,
  notificationStatus: "idle",
  notificationMessage: null,
};

class QualityStore {
  private state: QualityState = DEFAULT_STATE;
  private listeners = new Set<() => void>();

  getState = (): QualityState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(next: QualityState) {
    this.state = next;
    this.listeners.forEach((listener) => listener());
  }

  setFilters(filters: Partial<QualityFilters>) {
    this.set({
      ...this.state,
      filters: { ...this.state.filters, ...filters },
    });
  }

  resetFilters() {
    this.set({ ...this.state, filters: DEFAULT_FILTERS });
  }

  setGlanceMode(glanceMode: boolean) {
    this.set({ ...this.state, glanceMode });
  }

  setNotificationState(update: {
    enabled?: boolean;
    status?: NotificationStatus;
    message?: string | null;
  }) {
    this.set({
      ...this.state,
      notificationsEnabled: update.enabled ?? this.state.notificationsEnabled,
      notificationStatus: update.status ?? this.state.notificationStatus,
      notificationMessage:
        update.message === undefined ? this.state.notificationMessage : update.message,
    });
  }

  reset() {
    this.set(DEFAULT_STATE);
  }
}

export const qualityStore = new QualityStore();

export function useQualityState(): QualityState {
  return useSyncExternalStore(qualityStore.subscribe, qualityStore.getState);
}
