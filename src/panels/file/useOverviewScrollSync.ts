import { useEffect, useState } from "react";

export interface OverviewScrollSyncResult {
  topLine: number;
  visibleLineCount: number;
  viewportHeight: number;
  scrollProgress: number;
}

export function useOverviewScrollSync(
  bodyRef: React.RefObject<HTMLElement | null> | null | undefined,
  totalLines: number,
): OverviewScrollSyncResult {
  const [topLine, setTopLine] = useState(1);
  const [visibleLineCount, setVisibleLineCount] = useState(1);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [scrollProgress, setScrollProgress] = useState(0);

  useEffect(() => {
    const body = bodyRef?.current ?? null;
    if (!body || totalLines <= 0) {
      setTopLine(1);
      setVisibleLineCount(1);
      setViewportHeight(0);
      setScrollProgress(0);
      return;
    }

    let rafId: number | null = null;

    const compute = () => {
      rafId = null;
      const lineEls = Array.from(
        body.querySelectorAll<HTMLElement>("[data-line], [data-new-line]"),
      ).filter((el) => !el.closest(".file-overview-ruler"));
      if (lineEls.length === 0) {
        setTopLine(1);
        setVisibleLineCount(1);
        return;
      }
      const bodyRect = body.getBoundingClientRect();
      setViewportHeight(body.clientHeight || bodyRect.height);
      const maxScroll = Math.max(0, body.scrollHeight - body.clientHeight);
      setScrollProgress(maxScroll > 0 ? Math.min(1, Math.max(0, body.scrollTop / maxScroll)) : 0);
      let firstVisible = 1;
      let lastVisible = 1;
      let foundVisible = false;
      for (const el of lineEls) {
        const rect = el.getBoundingClientRect();
        if (rect.bottom > bodyRect.top && rect.top < bodyRect.bottom) {
          const raw = el.getAttribute("data-line") ?? el.getAttribute("data-new-line");
          const line = raw == null ? NaN : Number(raw);
          if (!Number.isNaN(line) && line > 0) {
            if (!foundVisible) firstVisible = line;
            lastVisible = line;
            foundVisible = true;
          }
        }
      }
      setTopLine(firstVisible);
      setVisibleLineCount(Math.max(1, lastVisible - firstVisible + 1));
    };

    const schedule = () => {
      if (rafId !== null) return;
      rafId = requestAnimationFrame(compute);
    };

    body.addEventListener("scroll", schedule, { passive: true });
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    ro?.observe(body);
    schedule();

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      body.removeEventListener("scroll", schedule);
      ro?.disconnect();
    };
  }, [bodyRef, totalLines]);

  return { topLine, visibleLineCount, viewportHeight, scrollProgress };
}
