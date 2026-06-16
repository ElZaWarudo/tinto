// Shared per-line Shiki highlighter hook, used by both the diff renderer and
// the full-file view. Lazy-loads the highlighter off the critical path; renders
// plain text until (or unless) it loads, and for unknown languages / oversized
// files. Returns a stable render fn that maps a line of source to colored spans.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { type HighlighterCore, highlightLine, loadHighlighter } from "./highlight";

export type RenderLine = (content: string) => ReactNode;

export function useLineHighlighter(lang: string | null, enabled: boolean): RenderLine {
  const [hl, setHl] = useState<HighlighterCore | null>(null);
  useEffect(() => {
    if (!enabled || !lang) return;
    let active = true;
    void loadHighlighter().then((h) => active && setHl(h));
    return () => {
      active = false;
    };
  }, [enabled, lang]);

  return useCallback(
    (content: string): ReactNode => {
      if (hl && lang && enabled) {
        const toks = highlightLine(hl, content, lang);
        if (toks) {
          return toks.map((t, i) => (
            <span key={i} style={t.color ? { color: t.color } : undefined}>
              {t.content}
            </span>
          ));
        }
      }
      return content;
    },
    [hl, lang, enabled],
  );
}
