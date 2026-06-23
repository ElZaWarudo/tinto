import { useEffect, useState, type RefObject } from "react";

interface OverviewScrollState {
  topLine: number;
  viewportPercent: number;
  viewportSizePercent: number;
}

function findRows(
  body: HTMLElement,
  targetAttribute: "data-line" | "data-new-line",
): HTMLElement[] {
  return [...body.querySelectorAll<HTMLElement>(`[${targetAttribute}]`)].filter(
    (row) => !row.closest(".file-overview-ruler"),
  );
}

export function useOverviewScrollSync(
  overviewRef: RefObject<HTMLElement | null>,
  totalLines: number,
  targetAttribute: "data-line" | "data-new-line",
): OverviewScrollState {
  const [state, setState] = useState<OverviewScrollState>({
    topLine: 1,
    viewportPercent: 0,
    viewportSizePercent: 100,
  });

  useEffect(() => {
    const body = overviewRef.current?.closest(".file-view__body") as HTMLElement | null;
    if (!body || totalLines <= 0) {
      setState({ topLine: 1, viewportPercent: 0, viewportSizePercent: 100 });
      return;
    }

    let frame = 0;
    const update = () => {
      frame = 0;
      const maxScroll = Math.max(1, body.scrollHeight - body.clientHeight);
      const viewportPercent = (body.scrollTop / maxScroll) * 100;
      const viewportSizePercent = Math.max(
        4,
        Math.min(100, (body.clientHeight / body.scrollHeight) * 100),
      );
      const bodyRect = body.getBoundingClientRect();
      const firstVisible = findRows(body, targetAttribute).find((row) => {
        const rect = row.getBoundingClientRect();
        return rect.bottom > bodyRect.top;
      });
      const topLine = firstVisible?.getAttribute(targetAttribute);
      setState({
        topLine: Math.max(1, Math.min(totalLines, Number(topLine) || 1)),
        viewportPercent,
        viewportSizePercent,
      });
    };

    const schedule = () => {
      if (frame) return;
      frame = requestAnimationFrame(update);
    };

    update();
    body.addEventListener("scroll", schedule, { passive: true });
    const resizeObserver =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    resizeObserver?.observe(body);

    return () => {
      body.removeEventListener("scroll", schedule);
      resizeObserver?.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [overviewRef, targetAttribute, totalLines]);

  return state;
}
