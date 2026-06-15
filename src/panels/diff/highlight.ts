// Lazy Shiki highlighter (RDM-008, D-008-2/KTD5). Uses shiki/core with the
// oniguruma engine and on-demand grammar/theme imports so the highlighter is a
// separate chunk loaded only when a diff is first shown — never on the critical
// path to first paint. Bounded by a size cap; unknown languages and oversized
// content fall back to plain monospace (the caller renders plain when this
// returns null).

import { createHighlighterCore, type HighlighterCore } from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";

export type { HighlighterCore };

/** Above this many bytes, skip highlighting (large-file responsiveness, R13). */
export const MAX_HIGHLIGHT_BYTES = 256 * 1024;

/** The theme that matches dockview's themeVisualStudio dark aesthetic. */
const THEME = "dark-plus";

// Extension → Shiki language id. A deliberately small set for the prototype;
// anything not here renders plain (still correct, just unstyled).
const LANG_BY_EXT: Record<string, string> = {
  ts: "typescript",
  tsx: "tsx",
  js: "javascript",
  jsx: "jsx",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  rs: "rust",
  py: "python",
  css: "css",
  html: "html",
  md: "markdown",
  sh: "shell",
  bash: "shell",
  toml: "toml",
  yaml: "yaml",
  yml: "yaml",
};

/** Shiki language id for a path, or null when the extension is unknown. */
export function languageFromPath(path: string): string | null {
  const dot = path.lastIndexOf(".");
  if (dot < 0) return null;
  return LANG_BY_EXT[path.slice(dot + 1).toLowerCase()] ?? null;
}

let highlighterPromise: Promise<HighlighterCore | null> | null = null;

/** Lazily create (once) the shared highlighter; null if it fails to load. */
export function loadHighlighter(): Promise<HighlighterCore | null> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterCore({
      themes: [import("shiki/themes/dark-plus.mjs")],
      langs: [
        import("shiki/langs/typescript.mjs"),
        import("shiki/langs/tsx.mjs"),
        import("shiki/langs/javascript.mjs"),
        import("shiki/langs/jsx.mjs"),
        import("shiki/langs/json.mjs"),
        import("shiki/langs/rust.mjs"),
        import("shiki/langs/python.mjs"),
        import("shiki/langs/css.mjs"),
        import("shiki/langs/html.mjs"),
        import("shiki/langs/markdown.mjs"),
        import("shiki/langs/shellscript.mjs"),
        import("shiki/langs/toml.mjs"),
        import("shiki/langs/yaml.mjs"),
      ],
      engine: createOnigurumaEngine(import("shiki/wasm")),
    }).catch(() => null);
  }
  return highlighterPromise;
}

/** One highlighted token: text + a CSS color (or undefined for default). */
export interface HlToken {
  content: string;
  color?: string;
}

/** Tokenize a single line of code. Returns null on any failure / unsupported
 * language so the caller falls back to plain text. */
export function highlightLine(
  highlighter: HighlighterCore,
  line: string,
  lang: string,
): HlToken[] | null {
  if (!highlighter.getLoadedLanguages().includes(lang)) return null;
  try {
    const { tokens } = highlighter.codeToTokens(line, { lang, theme: THEME });
    // Single line in → at most one row of tokens out.
    return (tokens[0] ?? []).map((t) => ({ content: t.content, color: t.color }));
  } catch {
    return null;
  }
}
