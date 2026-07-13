// Fixed VS Code-style menu bar (outside the dock area): brand, workbench
// switcher, and drop-down menus that condense every workbench/view action plus
// a "Proyectos" launcher (each repo opens its level-1 project tab). The per-
// project file explorer lives inside each project tab; this bar only carries
// actions.

import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { busStore, sortedRepoPaths, useBusState } from "../bus/store";
import { enableNotifications, disableNotifications } from "../qol/notifications";
import { qualityStore, useQualityState } from "../qol/state";
import { zoomStore } from "../qol/zoom";
import { useWorkspaceActions } from "../workspace/actions";
import tintoWordmarkDark from "../assets/brand/tinto-wordmark-dark.png";
import { autodetectFlow, switchWorkbench } from "./operations";
import { visibleWorkbenchNames } from "./recentWorkbenches";
import { AddonsManager } from "./AddonsManager";
import { KeyboardShortcuts } from "./KeyboardShortcuts";
import { ManageWorkbenchesDialog } from "./ManageWorkbenchesDialog";

type MenuId = "workbench" | "repos" | "projects" | "view" | "addons" | "help";

const MENU_IDS: MenuId[] = ["workbench", "repos", "projects", "view", "addons", "help"];

type MenuItemRole = "checkbox" | "radio";
type MenuFeedback = { tone: "status" | "error"; message: string };

function actionErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

function MenuItem({
  label,
  onSelect,
  close,
  testid,
  checked,
  checkRole,
  disabled = false,
}: {
  label: string;
  onSelect: () => void;
  close: () => void;
  testid?: string;
  checked?: boolean;
  checkRole?: MenuItemRole;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="menu__item"
      role={checkRole ? `menuitem${checkRole}` : "menuitem"}
      aria-checked={checkRole ? Boolean(checked) : undefined}
      disabled={disabled}
      tabIndex={-1}
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
  const [showAddons, setShowAddons] = useState(false);
  const [showManage, setShowManage] = useState(false);
  const [feedback, setFeedback] = useState<MenuFeedback | null>(null);
  const [switchingWorkbench, setSwitchingWorkbench] = useState<string | null>(null);
  const [activeTrigger, setActiveTrigger] = useState<MenuId>("workbench");
  const menubarRef = useRef<HTMLDivElement | null>(null);
  const pendingMenuFocus = useRef<"first" | "last" | null>(null);

  const active = config?.active ?? "";
  const rovingTrigger = !active && activeTrigger === "repos" ? "workbench" : activeTrigger;
  const workbenches = config?.workbenches ?? [];
  const visibleWorkbenches = visibleWorkbenchNames(
    workbenches.map((w) => w.name),
    active,
  );
  // Projects to open come from the live bus snapshot (same source as the
  // Dashboard cards), not the config — so the menu always matches what is
  // actually loaded, regardless of config-load timing.
  const projectPaths = sortedRepoPaths(busStore, state);
  const triggerFor = (id: MenuId) =>
    menubarRef.current?.querySelector<HTMLButtonElement>(`#menu-trigger-${id}`) ?? null;
  const menuItems = (id: MenuId): HTMLElement[] =>
    Array.from(
      menubarRef.current
        ?.querySelector<HTMLDivElement>(`#menu-popup-${id}`)
        ?.querySelectorAll<HTMLElement>(
          '[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]',
        ) ?? [],
    ).filter((item) => !(item instanceof HTMLButtonElement && item.disabled));

  const closeMenu = (restoreFocus = true) => {
    const trigger = open ? triggerFor(open) : null;
    pendingMenuFocus.current = null;
    setOpen(null);
    if (restoreFocus) trigger?.focus();
  };
  const close = () => closeMenu(true);

  const openMenu = (id: MenuId, focus: "first" | "last" = "first") => {
    if (triggerFor(id)?.disabled) return;
    setActiveTrigger(id);
    pendingMenuFocus.current = focus;
    if (open === id) {
      const items = menuItems(id);
      pendingMenuFocus.current = null;
      (focus === "last" ? items[items.length - 1] : items[0])?.focus();
      return;
    }
    setOpen(id);
  };

  useEffect(() => {
    if (!open || !pendingMenuFocus.current) return;
    const items = menuItems(open);
    const item = pendingMenuFocus.current === "last" ? items[items.length - 1] : items[0];
    pendingMenuFocus.current = null;
    item?.focus();
  }, [open]);

  const enabledTriggers = () =>
    MENU_IDS.filter((id) => {
      const trigger = triggerFor(id);
      return trigger && !trigger.disabled;
    });

  const moveTrigger = (
    current: MenuId,
    direction: -1 | 1 | "first" | "last",
    keepMenuOpen: boolean,
  ) => {
    const enabled = enabledTriggers();
    if (enabled.length === 0) return;
    const currentIndex = Math.max(0, enabled.indexOf(current));
    const next =
      direction === "first"
        ? enabled[0]
        : direction === "last"
          ? enabled[enabled.length - 1]
          : enabled[(currentIndex + direction + enabled.length) % enabled.length];
    if (!next) return;
    setActiveTrigger(next);
    if (keepMenuOpen) openMenu(next);
    else triggerFor(next)?.focus();
  };

  const handleTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, id: MenuId) => {
    switch (event.key) {
      case "ArrowRight":
        event.preventDefault();
        moveTrigger(id, 1, open !== null);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveTrigger(id, -1, open !== null);
        break;
      case "Home":
        event.preventDefault();
        moveTrigger(id, "first", open !== null);
        break;
      case "End":
        event.preventDefault();
        moveTrigger(id, "last", open !== null);
        break;
      case "ArrowDown":
        event.preventDefault();
        openMenu(id, "first");
        break;
      case "ArrowUp":
        event.preventDefault();
        openMenu(id, "last");
        break;
      case "Escape":
        if (open !== null) {
          event.preventDefault();
          closeMenu(true);
        }
        break;
    }
  };

  const handleMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>, id: MenuId) => {
    const items = menuItems(id);
    const currentIndex = items.indexOf(document.activeElement as HTMLElement);
    const focusItem = (index: number) => items[index]?.focus();

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        event.stopPropagation();
        focusItem(currentIndex < 0 ? 0 : (currentIndex + 1) % items.length);
        break;
      case "ArrowUp":
        event.preventDefault();
        event.stopPropagation();
        focusItem(
          currentIndex < 0 ? items.length - 1 : (currentIndex - 1 + items.length) % items.length,
        );
        break;
      case "Home":
        event.preventDefault();
        event.stopPropagation();
        focusItem(0);
        break;
      case "End":
        event.preventDefault();
        event.stopPropagation();
        focusItem(items.length - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        event.stopPropagation();
        moveTrigger(id, 1, true);
        break;
      case "ArrowLeft":
        event.preventDefault();
        event.stopPropagation();
        moveTrigger(id, -1, true);
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        closeMenu(true);
        break;
      case "Tab":
        closeMenu(true);
        break;
    }
  };

  const handleTriggerClick = (id: MenuId) => {
    if (open === id) closeMenu(false);
    else openMenu(id);
  };

  const runWorkbenchSwitch = (name: string) => {
    if (switchingWorkbench) return;
    setSwitchingWorkbench(name);
    setFeedback({ tone: "status", message: `Cambiando a ${name}…` });
    void Promise.resolve()
      .then(() => switchWorkbench(name, active || null))
      .then(() => setFeedback(null))
      .catch((error) =>
        setFeedback({
          tone: "error",
          message: actionErrorMessage(error, `No se pudo activar la workbench ${name}.`),
        }),
      )
      .finally(() => setSwitchingWorkbench(null));
  };

  const runAutodetect = () => {
    if (!active) return;
    setFeedback({ tone: "status", message: "Buscando repositorios…" });
    void Promise.resolve()
      .then(() => autodetectFlow(active))
      .then((result) => {
        if (!result) {
          setFeedback(null);
          return;
        }
        const skipped = result.failed ? `; ${result.failed} no se pudieron añadir` : "";
        setFeedback({
          tone: "status",
          message: `${result.added} de ${result.found} repositorios añadidos${skipped}.`,
        });
      })
      .catch((error) =>
        setFeedback({
          tone: "error",
          message: actionErrorMessage(error, "No se pudieron detectar repositorios."),
        }),
      );
  };

  return (
    <div className="menu-bar">
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

      <div ref={menubarRef} className="menu-bar__menus" role="menubar" aria-label="Barra de menús">
        <div className="menu" role="none">
          <button
            id="menu-trigger-workbench"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "workbench" ? 0 : -1}
            className={open === "workbench" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-workbench"
            aria-haspopup="menu"
            aria-expanded={open === "workbench"}
            aria-controls="menu-popup-workbench"
            onFocus={() => setActiveTrigger("workbench")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "workbench")}
            onClick={() => handleTriggerClick("workbench")}
          >
            Workbench
          </button>
          {open === "workbench" && (
            <div
              id="menu-popup-workbench"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-workbench"
              onKeyDown={(event) => handleMenuKeyDown(event, "workbench")}
            >
              {visibleWorkbenches.length === 0 ? (
                <div
                  className="menu__empty"
                  data-testid="workbench-empty"
                  role="menuitem"
                  aria-disabled="true"
                  tabIndex={-1}
                >
                  Sin workbenches.
                </div>
              ) : (
                visibleWorkbenches.map((name) => (
                  <MenuItem
                    key={name}
                    label={name}
                    testid={`workbench-recent-${name}`}
                    checked={name === active}
                    checkRole="radio"
                    disabled={switchingWorkbench !== null}
                    close={close}
                    onSelect={() => runWorkbenchSwitch(name)}
                  />
                ))
              )}
              <div className="menu__sep" role="separator" />
              <MenuItem
                label="Gestionar workbenches…"
                testid="workbench-manage"
                close={close}
                onSelect={() => setShowManage(true)}
              />
            </div>
          )}
        </div>

        <div className="menu" role="none">
          <button
            id="menu-trigger-repos"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "repos" ? 0 : -1}
            className={open === "repos" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-repos"
            aria-haspopup="menu"
            aria-expanded={open === "repos"}
            aria-controls="menu-popup-repos"
            disabled={!active}
            onFocus={() => setActiveTrigger("repos")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "repos")}
            onClick={() => handleTriggerClick("repos")}
          >
            Repos
          </button>
          {open === "repos" && (
            <div
              id="menu-popup-repos"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-repos"
              onKeyDown={(event) => handleMenuKeyDown(event, "repos")}
            >
              <MenuItem
                label="Agregar repositorio…"
                testid="add-repo"
                close={close}
                onSelect={() => active && addRepo()}
              />
              <MenuItem
                label="Detectar repositorios automáticamente…"
                testid="autodetect"
                close={close}
                onSelect={runAutodetect}
              />
            </div>
          )}
        </div>

        <div className="menu" role="none">
          <button
            id="menu-trigger-projects"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "projects" ? 0 : -1}
            className={open === "projects" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-projects"
            aria-haspopup="menu"
            aria-expanded={open === "projects"}
            aria-controls="menu-popup-projects"
            onFocus={() => setActiveTrigger("projects")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "projects")}
            onClick={() => handleTriggerClick("projects")}
          >
            Proyectos
          </button>
          {open === "projects" && (
            <div
              id="menu-popup-projects"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-projects"
              onKeyDown={(event) => handleMenuKeyDown(event, "projects")}
            >
              {projectPaths.length === 0 ? (
                <div
                  className="menu__empty"
                  data-testid="projects-empty"
                  role="menuitem"
                  aria-disabled="true"
                  tabIndex={-1}
                >
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

        <div className="menu" role="none">
          <button
            id="menu-trigger-view"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "view" ? 0 : -1}
            className={open === "view" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-view"
            aria-haspopup="menu"
            aria-expanded={open === "view"}
            aria-controls="menu-popup-view"
            onFocus={() => setActiveTrigger("view")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "view")}
            onClick={() => handleTriggerClick("view")}
          >
            Ver
          </button>
          {open === "view" && (
            <div
              id="menu-popup-view"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-view"
              onKeyDown={(event) => handleMenuKeyDown(event, "view")}
            >
              <MenuItem
                label="Abrir resumen"
                testid="open-dashboard"
                close={close}
                onSelect={openDashboard}
              />
              <MenuItem
                label="Abrir cronología"
                testid="open-timeline"
                close={close}
                onSelect={openTimeline}
              />
              <div className="menu__sep" role="separator" />
              <MenuItem
                label="Aumentar texto del archivo  (Ctrl +)"
                testid="zoom-in"
                close={close}
                onSelect={() => zoomStore.zoomIn()}
              />
              <MenuItem
                label="Reducir texto del archivo  (Ctrl −)"
                testid="zoom-out"
                close={close}
                onSelect={() => zoomStore.zoomOut()}
              />
              <MenuItem
                label="Restablecer texto del archivo  (Ctrl 0)"
                testid="zoom-reset"
                close={close}
                onSelect={() => zoomStore.reset()}
              />
              <div className="menu__sep" role="separator" />
              <MenuItem
                label="Vista rápida"
                testid="qol-glance"
                checked={quality.glanceMode}
                checkRole="checkbox"
                close={close}
                onSelect={() => qualityStore.setGlanceMode(!quality.glanceMode)}
              />
              <div className="menu__sep" role="separator" />
              <MenuItem
                label="Notificaciones"
                testid="qol-notifications"
                checked={quality.notificationsEnabled}
                checkRole="checkbox"
                close={close}
                onSelect={() =>
                  quality.notificationsEnabled ? disableNotifications() : void enableNotifications()
                }
              />
            </div>
          )}
        </div>

        <div className="menu" role="none">
          <button
            id="menu-trigger-addons"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "addons" ? 0 : -1}
            className={open === "addons" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-addons"
            aria-haspopup="menu"
            aria-expanded={open === "addons"}
            aria-controls="menu-popup-addons"
            onFocus={() => setActiveTrigger("addons")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "addons")}
            onClick={() => handleTriggerClick("addons")}
          >
            Complementos
          </button>
          {open === "addons" && (
            <div
              id="menu-popup-addons"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-addons"
              onKeyDown={(event) => handleMenuKeyDown(event, "addons")}
            >
              <MenuItem
                label="Gestionar complementos"
                testid="manage-addons"
                close={close}
                onSelect={() => setShowAddons(true)}
              />
            </div>
          )}
        </div>

        <div className="menu" role="none">
          <button
            id="menu-trigger-help"
            type="button"
            role="menuitem"
            tabIndex={rovingTrigger === "help" ? 0 : -1}
            className={open === "help" ? "menu__trigger menu__trigger--open" : "menu__trigger"}
            data-testid="menu-help"
            aria-haspopup="menu"
            aria-expanded={open === "help"}
            aria-controls="menu-popup-help"
            onFocus={() => setActiveTrigger("help")}
            onKeyDown={(event) => handleTriggerKeyDown(event, "help")}
            onClick={() => handleTriggerClick("help")}
          >
            Ayuda
          </button>
          {open === "help" && (
            <div
              id="menu-popup-help"
              className="menu__list"
              role="menu"
              aria-labelledby="menu-trigger-help"
              onKeyDown={(event) => handleMenuKeyDown(event, "help")}
            >
              <MenuItem
                label="Atajos de teclado"
                testid="show-shortcuts"
                close={close}
                onSelect={() => setShowShortcuts(true)}
              />
            </div>
          )}
        </div>
      </div>

      <span className="menu-bar__spacer" />

      {feedback && (
        <span
          className={`menu-bar__feedback menu-bar__feedback--${feedback.tone}`}
          role={feedback.tone === "error" ? "alert" : "status"}
          aria-live={feedback.tone === "error" ? "assertive" : "polite"}
          data-testid="menu-action-feedback"
        >
          <span>{feedback.message}</span>
          <button type="button" aria-label="Cerrar mensaje" onClick={() => setFeedback(null)}>
            ×
          </button>
        </span>
      )}

      {quality.notificationStatus === "denied" || quality.notificationStatus === "unavailable" ? (
        <span className="menu-bar__notice" data-testid="qol-notification-status">
          {quality.notificationStatus === "denied" ? "Notificaciones bloqueadas" : "Sin avisos"}
        </span>
      ) : null}

      <span
        className={watching.available ? "watch watch--ok" : "watch watch--bad"}
        title={watching.reason ?? "Observación de archivos activa"}
        role="status"
        aria-label={
          watching.available
            ? "Observación de archivos activa"
            : `Observación degradada${watching.reason ? `: ${watching.reason}` : ""}`
        }
        data-testid="watch-indicator"
      >
        {watching.available ? "● observando" : "○ degradado"}
      </span>

      {/* Click-away backdrop: closes any open menu. */}
      {open !== null && (
        <button
          type="button"
          className="menu__backdrop"
          aria-hidden="true"
          tabIndex={-1}
          data-testid="menu-backdrop"
          onClick={() => closeMenu(false)}
        />
      )}

      {showShortcuts && <KeyboardShortcuts onClose={() => setShowShortcuts(false)} />}
      {showAddons && <AddonsManager onClose={() => setShowAddons(false)} />}
      {showManage && config && (
        <ManageWorkbenchesDialog
          config={config}
          onClose={() => setShowManage(false)}
          onCreated={() => {
            setShowManage(false);
            openDashboard({ closeAll: true });
          }}
        />
      )}
    </div>
  );
}
