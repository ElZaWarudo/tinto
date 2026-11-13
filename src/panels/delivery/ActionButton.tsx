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
import { errorText } from "./errorText";
import { Icon } from "../../workbench/Icon";

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
/** How long the check is seen at least before a refresh may remove the button. */
const CHECK_MS = 160;

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
  // From the click until its refresh has ended: a second click in between
  // would run the action again on what the panel still shows.
  const busy = useRef(false);
  // The check and the failure mark go away by themselves, the check only
  // once the refresh has ended.
  const [settled, setSettled] = useState(true);
  useEffect(() => {
    if (state === "failed" || (state === "done" && settled)) {
      const timer = setTimeout(() => setState("idle"), DONE_MS);
      return () => clearTimeout(timer);
    }
  }, [state, settled]);

  const working = state === "working";
  const click = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    if (busy.current) return;
    // A submit button still asks the form for its required fields first.
    const form = type === "submit" ? event.currentTarget.form : null;
    if (form && !form.reportValidity()) return;
    busy.current = true;
    setError(null);
    setSettled(false);
    setState("working");
    group?.setWorking(true);
    try {
      const result = await onAction();
      if (result === false) {
        if (mounted.current) setState("idle");
        return;
      }
      // The check shows first: the refresh may remove this card.
      if (mounted.current) setState("done");
      await new Promise((resolve) => setTimeout(resolve, CHECK_MS));
      await refresh();
    } catch (cause) {
      if (mounted.current) {
        setError(errorText(cause));
        setState("failed");
      }
    } finally {
      busy.current = false;
      if (mounted.current) setSettled(true);
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
            {working ? (
              <span className="delivery-action__spinner" />
            ) : (
              <Icon name="check" className="delivery-icon" />
            )}
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
