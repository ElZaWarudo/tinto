import { useEffect, useRef, type RefObject } from "react";

/** The overlays open in the panel, outermost first. */
const opened: { surface: RefObject<HTMLElement | null>; close: () => void }[] = [];

/** One overlay at a time: opening one closes the others, unless it opens from
 *  inside one of them (a menu in a card), which stays under it. A press
 *  outside it, its control and the ones opened from it closes it. */
export function useOneOverlay(
  surface: RefObject<HTMLElement | null>,
  from: Element | null,
  onClose: () => void,
) {
  const close = useRef(onClose);
  useEffect(() => {
    close.current = onClose;
  });
  useEffect(() => {
    const entry = { surface, close: () => close.current() };
    for (const other of [...opened]) {
      if (!from || !other.surface.current?.contains(from)) other.close();
    }
    opened.push(entry);
    const press = (event: MouseEvent) => {
      const target = event.target as Node;
      if (from?.contains(target)) return;
      const above = opened.slice(opened.indexOf(entry));
      if (above.some((one) => one.surface.current?.contains(target))) return;
      entry.close();
    };
    document.addEventListener("mousedown", press);
    return () => {
      document.removeEventListener("mousedown", press);
      opened.splice(opened.indexOf(entry), 1);
    };
    // Once, when it opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
