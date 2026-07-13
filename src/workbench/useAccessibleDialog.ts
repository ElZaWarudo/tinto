import { useEffectEvent, useLayoutEffect, useRef, type RefObject } from "react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "iframe",
  "object",
  "embed",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

const openDialogs: symbol[] = [];

interface AccessibleDialogOptions {
  onClose: () => void;
  onEscape?: () => void;
  initialFocusRef?: { readonly current: HTMLElement | null };
}

interface BackgroundState {
  element: HTMLElement;
  hadInert: boolean;
  ariaHidden: string | null;
}

interface RestoreTargets {
  primary: HTMLElement | null;
  fallback: HTMLElement | null;
}

function captureRestoreTargets(): RestoreTargets {
  const primary = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const menu = primary?.closest<HTMLElement>("[role='menu']");
  const fallback =
    menu?.parentElement?.querySelector<HTMLElement>("button[aria-haspopup='menu']") ?? null;
  return { primary, fallback };
}

function isAvailable(element: HTMLElement): boolean {
  if (!element.isConnected || element.closest("[hidden], [aria-hidden='true'], [inert]")) {
    return false;
  }

  const style = window.getComputedStyle(element);
  return style.display !== "none" && style.visibility !== "hidden";
}

function focusableElements(dialog: HTMLElement): HTMLElement[] {
  return Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(isAvailable);
}

function makeBackgroundUnavailable(dialog: HTMLElement): BackgroundState[] {
  const states: BackgroundState[] = [];
  let current: HTMLElement | null = dialog;

  while (current && current !== document.body) {
    const parent: HTMLElement | null = current.parentElement;
    if (!parent) break;

    for (const sibling of Array.from(parent.children)) {
      if (!(sibling instanceof HTMLElement) || sibling === current) continue;
      states.push({
        element: sibling,
        hadInert: sibling.hasAttribute("inert"),
        ariaHidden: sibling.getAttribute("aria-hidden"),
      });
      sibling.setAttribute("inert", "");
      sibling.setAttribute("aria-hidden", "true");
    }

    current = parent;
  }

  return states;
}

function restoreBackground(states: BackgroundState[]): void {
  for (const { element, hadInert, ariaHidden } of states.reverse()) {
    if (!hadInert) element.removeAttribute("inert");
    if (ariaHidden === null) element.removeAttribute("aria-hidden");
    else element.setAttribute("aria-hidden", ariaHidden);
  }
}

function trapTab(event: KeyboardEvent, dialog: HTMLElement): void {
  const focusable = focusableElements(dialog);
  if (focusable.length === 0) {
    event.preventDefault();
    dialog.focus();
    return;
  }

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;

  if (event.shiftKey && (active === first || !dialog.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !dialog.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}

/**
 * Applies the shared keyboard and focus contract for an aria-modal dialog.
 * The dialog stays in the existing DOM position so callers keep ownership of
 * backdrop clicks and visual styling.
 */
export function useAccessibleDialog<T extends HTMLElement = HTMLDivElement>({
  onClose,
  onEscape,
  initialFocusRef,
}: AccessibleDialogOptions): RefObject<T | null> {
  const dialogRef = useRef<T>(null);
  const instanceRef = useRef(Symbol("accessible-dialog"));
  const restoreTargetsRef = useRef(captureRestoreTargets());
  const closeFromEscape = useEffectEvent(() => (onEscape ?? onClose)());

  useLayoutEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const instance = instanceRef.current;
    const restoreTargets = restoreTargetsRef.current;
    const previousTabIndex = dialog.getAttribute("tabindex");
    if (previousTabIndex === null) dialog.tabIndex = -1;

    openDialogs.push(instance);

    const requestedFocus = initialFocusRef?.current;
    const initialFocus =
      requestedFocus && dialog.contains(requestedFocus) && isAvailable(requestedFocus)
        ? requestedFocus
        : (focusableElements(dialog)[0] ?? dialog);
    initialFocus.focus();

    const background = makeBackgroundUnavailable(dialog);
    const handleKeyDown = (event: KeyboardEvent) => {
      if (openDialogs[openDialogs.length - 1] !== instance || event.defaultPrevented) return;

      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closeFromEscape();
      } else if (event.key === "Tab") {
        trapTab(event, dialog);
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      const index = openDialogs.lastIndexOf(instance);
      if (index >= 0) openDialogs.splice(index, 1);
      restoreBackground(background);

      if (previousTabIndex === null) dialog.removeAttribute("tabindex");
      else dialog.setAttribute("tabindex", previousTabIndex);

      const { primary, fallback } = restoreTargets;
      if (primary?.isConnected) primary.focus();
      else if (fallback?.isConnected) fallback.focus();
    };
  }, [initialFocusRef]);

  return dialogRef;
}
