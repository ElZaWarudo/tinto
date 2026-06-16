// Diff renderer (RDM-008, D-008-1): renders a structured FileDiff as either an
// inline (stacked removed→added) or side-by-side (old left / new right) view,
// from one data source. Syntax highlighting (Shiki) layers on after the
// structure paints and is bounded by a size cap; binary and oversized files
// fall back to a placeholder / plain monospace.

import type { DiffHunk, DiffLine, FileDiff } from "../../bus/contract";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { useLineHighlighter, type RenderLine } from "./lineHighlighter";

export type DiffMode = "inline" | "side-by-side";

function diffSize(diff: FileDiff): number {
  let n = 0;
  for (const h of diff.hunks) for (const l of h.lines) n += l.content.length;
  return n;
}

const SIGN: Record<DiffLine["kind"], string> = { Added: "+", Removed: "-", Context: " " };

export function DiffView({ diff, mode }: { diff: FileDiff; mode: DiffMode }) {
  const oversized = diffSize(diff) > MAX_HIGHLIGHT_BYTES;
  const lang = languageFromPath(diff.path);
  const render = useLineHighlighter(lang, !diff.is_binary && !oversized);

  if (diff.is_binary) {
    return (
      <div className="diff-view diff-view--binary" data-testid="diff-binary">
        Binary file — no text diff.
      </div>
    );
  }

  return (
    <div className={`diff-view diff-view--${mode}`} data-testid="diff-view">
      {oversized && (
        <div className="diff-view__notice" data-testid="diff-large">
          Large file — syntax highlighting disabled.
        </div>
      )}
      {diff.hunks.length === 0 ? (
        <div className="diff-view__notice" data-testid="diff-no-hunks">
          No textual changes.
        </div>
      ) : mode === "inline" ? (
        diff.hunks.map((h, i) => <InlineHunk key={i} hunk={h} render={render} />)
      ) : (
        diff.hunks.map((h, i) => <SplitHunk key={i} hunk={h} render={render} />)
      )}
    </div>
  );
}

function HunkHeader({ hunk }: { hunk: DiffHunk }) {
  return (
    <div className="diff-hunk-header" data-testid="hunk-header">
      @@ -{hunk.old_start} +{hunk.new_start} @@
    </div>
  );
}

function InlineHunk({ hunk, render }: { hunk: DiffHunk; render: RenderLine }) {
  return (
    <div className="diff-hunk">
      <HunkHeader hunk={hunk} />
      {hunk.lines.map((l, i) => (
        <div key={i} className={`diff-line diff-line--${l.kind.toLowerCase()}`}>
          <span className="diff-gutter">{l.old_lineno ?? ""}</span>
          <span className="diff-gutter">{l.new_lineno ?? ""}</span>
          <span className="diff-sign">{SIGN[l.kind]}</span>
          <code className="diff-content">{render(l.content)}</code>
        </div>
      ))}
    </div>
  );
}

function SplitHunk({ hunk, render }: { hunk: DiffHunk; render: RenderLine }) {
  // Left = old file slice (context + removed); right = new (context + added).
  const left = hunk.lines.filter((l) => l.kind !== "Added");
  const right = hunk.lines.filter((l) => l.kind !== "Removed");
  return (
    <div className="diff-hunk">
      <HunkHeader hunk={hunk} />
      <div className="diff-split">
        <div className="diff-side diff-side--old" data-testid="diff-old">
          {left.map((l, i) => (
            <div key={i} className={`diff-line diff-line--${l.kind.toLowerCase()}`}>
              <span className="diff-gutter">{l.old_lineno ?? ""}</span>
              <code className="diff-content">{render(l.content)}</code>
            </div>
          ))}
        </div>
        <div className="diff-side diff-side--new" data-testid="diff-new">
          {right.map((l, i) => (
            <div key={i} className={`diff-line diff-line--${l.kind.toLowerCase()}`}>
              <span className="diff-gutter">{l.new_lineno ?? ""}</span>
              <code className="diff-content">{render(l.content)}</code>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
