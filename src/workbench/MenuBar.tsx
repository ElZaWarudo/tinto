// Fixed VS Code-style menu bar (outside the dock area): brand, workbench
// switcher, and drop-down menus that condense every workbench/view action plus
// a "Proyectos" launcher (each repo opens its level-1 project tab). The per-
// project file explorer lives inside each project tab; this bar only carries
// actions.

import { useState } from "react";
import { busStore, sortedRepoPaths, useBusState } from "../bus/store";
import { enableNotifications, disableNotifications } from "../qol/notifications";
import { qualityStore, useQualityState } from "../qol/state";
import { zoomStore } from "../qol/zoom";
import { useWorkspaceActions } from "../workspace/actions";
import tintoWordmarkDark from "../assets/brand/tinto-wordmark-dark.png";
import { autodetectFlow, switchWorkbench } from "./operations";
import { KeyboardShortcuts } from "./KeyboardShortcuts";

type MenuId = "repos" | "projects" | "view" | "help";

function MenuItem({
  label,
  onSelect,
  close,
  testid,
  checked,
}: {
  label: string;
  onSelect: () => void;
  close: () => void;
  testid?: string;
  checked?: boolean;
}) {
  return (
    <button
      type="button"
      className="menu__item"
      role="menuitem"
      data-testid={testid}
      onClick={() => {
        onSelect();
        close();
      }}
    >
      <span className="menu__check">{checked ? "✓" : ""}</span>
      <span>{label}</span>
    </button>
  );
}

export function MenuBar() {
  const state = useBusState();
  const { config, watching } = state;
  const quality = useQualityState();
  const { openTimeline, openDashboard, openRepo, addRepo } = useWorkspaceActions();
  const [open, setOpen] = useState<MenuId | null>(null);
  const [showShortcuts, setShowShortcuts] = useState(false);

  const active = config?.active ?? "";
  const workbenches = config?.workbenches ?? [];
  // Projects to open come from the live bus snapshot (same source as the
  // Dashboard cards), not the config — so the menu always matches what is
  // actually loaded, regardless of config-load timing.
  const projectPaths = sortedRepoPaths(busStore, state);
  const close = () => setOpen(null);
  const toggle = (id: MenuId) => setOpen((cur) => (cur === id ? null : id));

  return (
    <div className="menu-bar" role="menubar">
      <span className="menu-bar__brand">
        <img
          className="menu-bar__brand-img"
          src={tintoWordmarkDark}
          alt="Tinto"
          onError={(event) => {
            event.currentTarget.style.display = "none";
          }}
        />
        <span className="menu-bar__brand-fallback" aria-hidden="true">
          Tinto
        </span>
      </span>

      {workbenches.length > 1 && (
        <select
          className="menu-bar__switcher"
          data-testid="wb-switcher"
          value={active}
          onChange={(e) => void switchWorkbench(e.target.value, active || null)}
          title="Cambiar espacio de trabajo"
        >
          {workbenches.map((w) => (
            <option key={w.name} value={w.name}>
              {w.name}
            </option>
          ))}
        </select>
      )}

      <div className="menu">
        <button
          type="button"
          className={open === "repos" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
          data-testid="menu-repos"
          aria-haspopup="menu"
          aria-expanded={open === "repos"}
          onClick={() => active && toggle("repos")}
        >
          Repos
        </button>
        {open === "repos" && (
          <div className="menu__list" role="menu">
            <MenuItem
              label="Add repo…"
              testid="add-repo"
              close={close}
              onSelect={() => active && addRepo()}
            />
            <MenuItem
              label="Auto-detect repos…"
              testid="autodetect"
              close={close}
              onSelect={() => active && void autodetectFlow(active)}
            />
          </div>
        )}
      </div>

      <div className="menu">
        <button
          type="button"
          className={open === "projects" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
          data-testid="menu-projects"
          aria-haspopup="menu"
          aria-expanded={open === "projects"}
          onClick={() => toggle("projects")}
        >
          Proyectos
        </button>
        {open === "projects" && (
          <div className="menu__list" role="menu">
            {projectPaths.length === 0 ? (
              <div className="menu__empty" data-testid="projects-empty">
                No hay proyectos en este workbench.
              </div>
            ) : (
              projectPaths.map((p) => (
                <MenuItem
                  key={p}
                  label={busStore.displayName(p)}
                  testid={`open-project-${p}`}
                  close={close}
                  onSelect={() => openRepo(p)}
                />
              ))
            )}
          </div>
        )}
      </div>

      <div className="menu">
        <button
          type="button"
          className={open === "view" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
          data-testid="menu-view"
          aria-haspopup="menu"
          aria-expanded={open === "view"}
          onClick={() => toggle("view")}
        >
          Ver
        </button>
        {open === "view" && (
          <div className="menu__list" role="menu">
            <MenuItem
              label="Abrir Dashboard"
              testid="open-dashboard"
              close={close}
              onSelect={openDashboard}
            />
            <MenuItem
              label="Abrir Timeline"
              testid="open-timeline"
              close={close}
              onSelect={openTimeline}
            />
            <div className="menu__sep" role="separator" />
            <MenuItem
              label="Aumentar texto  (Ctrl +)"
              testid="zoom-in"
              close={close}
              onSelect={() => zoomStore.zoomIn()}
            />
            <MenuItem
              label="Reducir texto  (Ctrl −)"
              testid="zoom-out"
              close={close}
              onSelect={() => zoomStore.zoomOut()}
            />
            <MenuItem
              label="Restablecer tamaño  (Ctrl 0)"
              testid="zoom-reset"
              close={close}
              onSelect={() => zoomStore.reset()}
            />
            <div className="menu__sep" role="separator" />
            <MenuItem
              label="Glance mode"
              testid="qol-glance"
              checked={quality.glanceMode}
              close={close}
              onSelect={() => qualityStore.setGlanceMode(!quality.glanceMode)}
            />
            <div className="menu__sep" role="separator" />
            <MenuItem
              label="Notificaciones"
              testid="qol-notifications"
              checked={quality.notificationsEnabled}
              close={close}
              onSelect={() =>
                quality.notificationsEnabled ? disableNotifications() : void enableNotifications()
              }
            />
          </div>
        )}
      </div>

      <div className="menu">
        <button
          type="button"
          className={open === "help" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
          data-testid="menu-help"
          aria-haspopup="menu"
          aria-expanded={open === "help"}
          onClick={() => toggle("help")}
        >
          Ayuda
        </button>
        {open === "help" && (
          <div className="menu__list" role="menu">
            <MenuItem
              label="Atajos de teclado"
              testid="show-shortcuts"
              close={close}
              onSelect={() => setShowShortcuts(true)}
            />
          </div>
        )}
      </div>

      <span className="menu-bar__spacer" />

      {quality.notificationStatus === "denied" || quality.notificationStatus === "unavailable" ? (
        <span className="menu-bar__notice" data-testid="qol-notification-status">
          {quality.notificationStatus}
        </span>
      ) : null}

      <span
        className={watching.available ? "watch watch--ok" : "watch watch--bad"}
        title={watching.reason ?? "watching"}
        data-testid="watch-indicator"
      >
        {watching.available ? "● watching" : "○ degraded"}
      </span>

      {/* Click-away backdrop: closes any open menu. */}
      {open !== null && (
        <button
          type="button"
          className="menu__backdrop"
          aria-hidden="true"
          tabIndex={-1}
          data-testid="menu-backdrop"
          onClick={close}
        />
      )}

      {showShortcuts && <KeyboardShortcuts onClose={() => setShowShortcuts(false)} />}
    </div>
  );
}
