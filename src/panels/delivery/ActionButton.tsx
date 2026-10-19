// A Delivery button that runs something: it shows its own progress and how it
// ended, and only blocks itself and the buttons of its group, never the panel.

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type MouseEvent,
  type ReactNode,
} from "react";
import { DeliveryRefreshContext } from "./deliveryRefresh";

type Group = { working: boolean; setWorking: (working: boolean) => void };
const ActionGroupContext = createContext<Group | null>(null);

/** Buttons about the same thing (one approval, one job): while one runs, the others wait. */
export function ActionGroup({ children }: { children: ReactNode }) {
  const [working, setWorking] = useState(false);
  return (
    <ActionGroupContext.Provider value={{ working, setWorking }}>
      {children}
    </ActionGroupContext.Provider>
  );
}

type State = "idle" | "working" | "done" | "failed";

/** How long the check stays once the action worked. */
const DONE_MS = 1200;

function errorText(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  return typeof cause === "string" ? cause : String(cause);
}

export function ActionButton({
  onAction,
  className,
  disabled,
  children,
  type = "button",
  ...rest
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onClick" | "type"> & {
  /**
   * The work. Resolving to `false` means it did not happen (the user
   * cancelled a confirmation); throwing shows why next to the button.
   */
  onAction: () => Promise<unknown>;
  /** "submit" also runs on Enter in its form, which clicks it. */
  type?: "button" | "submit";
  children: ReactNode;
}) {
  const refresh = useContext(DeliveryRefreshContext);
  const group = useContext(ActionGroupContext);
  const [state, setState] = useState<State>("idle");
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (state !== "done" && state !== "failed") return;
    const timer = setTimeout(() => setState("idle"), DONE_MS);
    return () => clearTimeout(timer);
  }, [state]);

  const working = state === "working";
  const click = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (working) return;
    // A submit button still asks the form for its required fields first.
    const form = type === "submit" ? event.currentTarget.form : null;
    if (form && !form.reportValidity()) return;
    setError(null);
    setState("working");
    group?.setWorking(true);
    try {
      const result = await onAction();
      if (result !== false) await refresh();
      if (mounted.current) setState(result === false ? "idle" : "done");
    } catch (cause) {
      if (mounted.current) {
        setError(errorText(cause));
        setState("failed");
      }
    } finally {
      group?.setWorking(false);
    }
  };

  return (
    <>
      <button
        {...rest}
        type={type}
        className={`delivery-action delivery-action--${state}${className ? ` ${className}` : ""}`}
        disabled={disabled || (group?.working && !working)}
        aria-busy={working || undefined}
        onClick={(event) => void click(event)}
      >
        <span className="delivery-action__label">{children}</span>
        {(working || state === "done") && (
          <span className="delivery-action__status" aria-hidden="true">
            {working ? <span className="delivery-action__spinner" /> : "✓"}
          </span>
        )}
      </button>
      {error && (
        <span className="delivery-action__error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}
