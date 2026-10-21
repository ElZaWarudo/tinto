// Theme picker in the menu bar: the trigger shows the theme in effect, and the
// panel lays every theme out as a swatch painted with that theme's own tokens
// (the [data-theme] blocks in App.css match any element). Choosing applies at
// once; arrow keys move the choice like any radio group, Escape closes.

import { useEffect, useRef, type KeyboardEvent } from "react";
import {
  THEME_OPTIONS,
  themeStore,
  useResolvedTheme,
  useThemeSetting,
  type ThemeSetting,
} from "../qol/theme";

function ThemeSwatch({ theme }: { theme: ThemeSetting }) {
  // "Según el sistema" previews both themes it can resolve to.
  if (theme === "system") {
    return (
      <span className="theme-swatch theme-swatch--split" aria-hidden="true">
        <ThemeSwatch theme="dark" />
        <ThemeSwatch theme="light" />
      </span>
    );
  }
  return (
    <span className="theme-swatch" data-theme={theme} aria-hidden="true">
      <span className="theme-swatch__bar" />
      <span className="theme-swatch__body">
        <span className="theme-swatch__line theme-swatch__line--strong" />
        <span className="theme-swatch__line" />
        <span className="theme-swatch__accent" />
      </span>
    </span>
  );
}

export function ThemePicker({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const setting = useThemeSetting();
  const resolved = useResolvedTheme();
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const groupRef = useRef<HTMLDivElement | null>(null);
  const current = THEME_OPTIONS.find((option) => option.value === setting) ?? THEME_OPTIONS[0];

  useEffect(() => {
    if (open) groupRef.current?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }, [open]);

  const close = () => {
    onOpenChange(false);
    triggerRef.current?.focus();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
    if (!step) return;
    event.preventDefault();
    const index = THEME_OPTIONS.findIndex((option) => option.value === setting);
    const next = THEME_OPTIONS[(index + step + THEME_OPTIONS.length) % THEME_OPTIONS.length];
    themeStore.set(next.value);
    groupRef.current?.querySelector<HTMLElement>(`[data-value="${next.value}"]`)?.focus();
  };

  return (
    <span className="theme-picker">
      <button
        ref={triggerRef}
        type="button"
        className={
          open ? "theme-picker__trigger theme-picker__trigger--open" : "theme-picker__trigger"
        }
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Cambiar el tema"
        data-testid="theme-picker-trigger"
        onClick={() => onOpenChange(!open)}
      >
        <ThemeSwatch theme={resolved} />
        <span className="theme-picker__trigger-label">{current.label}</span>
      </button>
      {open && (
        <>
          <button
            type="button"
            className="menu__backdrop"
            aria-hidden="true"
            tabIndex={-1}
            data-testid="theme-picker-backdrop"
            onClick={() => onOpenChange(false)}
          />
          <div
            className="theme-picker__panel"
            role="dialog"
            aria-labelledby="theme-picker-title"
            data-testid="theme-picker"
            onKeyDown={handleKeyDown}
          >
            <p id="theme-picker-title" className="theme-picker__title">
              Tema
            </p>
            <div
              ref={groupRef}
              className="theme-picker__options"
              role="radiogroup"
              aria-labelledby="theme-picker-title"
            >
              {THEME_OPTIONS.map((option) => {
                const checked = option.value === setting;
                return (
                  <button
                    key={option.value}
                    type="button"
                    role="radio"
                    aria-checked={checked}
                    tabIndex={checked ? 0 : -1}
                    className="theme-picker__option"
                    data-value={option.value}
                    data-testid={`theme-option-${option.value}`}
                    onClick={() => themeStore.set(option.value)}
                  >
                    <ThemeSwatch theme={option.value} />
                    <span className="theme-picker__option-label">{option.label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        </>
      )}
    </span>
  );
}
