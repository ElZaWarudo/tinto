// One node in a file tree: a collapsible folder (single click toggles) or a
// file. File interaction follows VS Code: a single click PREVIEWS (onOpen with
// pin=false), a double click PINS (pin=true); Enter pins, Space previews. The
// active file row is highlighted. Shared by the in-project explorer.

import { useState, type CSSProperties, type MouseEvent } from "react";
import type { RepoDelta } from "../../bus/contract";
import { getPathSignals } from "../../bus/store";
import { SignalBadges } from "../SignalBadges";
import type { TreeNode } from "./fileTree";

const MARK: Record<string, string> = { staged: "S", modified: "M", untracked: "U" };

function fileIconShape(kind: string): string {
  switch (kind) {
    case "react":
      return "atom";
    case "rust":
    case "config":
      return "gear";
    case "python":
      return "blocks";
    case "json":
      return "braces";
    case "markdown":
      return "markdown";
    case "css":
      return "palette";
    case "html":
    case "xml":
      return "markup";
    case "yaml":
    case "toml":
      return "sliders";
    case "shell":
      return "terminal";
    case "image":
      return "image";
    case "pdf":
      return "pdf";
    case "git":
      return "branch";
    case "test":
      return "flask";
    case "npm":
      return "box";
    case "lock":
    case "env":
      return "lock";
    case "docker":
      return "containers";
    case "database":
      return "database";
    case "archive":
      return "archive";
    case "font":
      return "font";
    case "text":
      return "text";
    default:
      return "code";
  }
}

function fileIconKind(name: string): string {
  const lower = name.toLowerCase();
  if (lower === "dockerfile" || lower === "docker-compose.yml" || lower === "docker-compose.yaml") {
    return "docker";
  }
  if (lower === "package.json") return "npm";
  if (lower === "package-lock.json" || lower === "pnpm-lock.yaml" || lower === "yarn.lock") {
    return "lock";
  }
  if (lower === "cargo.toml") return "toml";
  if (lower === "cargo.lock") return "lock";
  if (lower === ".env" || lower.startsWith(".env.")) return "env";
  if (lower === ".gitignore" || lower === ".gitattributes") return "git";
  if (lower.endsWith(".d.ts")) return "types";
  if (lower.endsWith(".test.ts") || lower.endsWith(".test.tsx")) return "test";
  if (
    lower.endsWith(".config.ts") ||
    lower.endsWith(".config.js") ||
    lower.endsWith(".config.mjs")
  ) {
    return "config";
  }
  const ext = lower.includes(".") ? lower.slice(lower.lastIndexOf(".") + 1) : "";
  switch (ext) {
    case "ts":
      return "typescript";
    case "tsx":
    case "jsx":
      return "react";
    case "js":
    case "mjs":
    case "cjs":
      return "javascript";
    case "rs":
      return "rust";
    case "py":
      return "python";
    case "go":
      return "go";
    case "rb":
      return "ruby";
    case "java":
      return "java";
    case "c":
    case "h":
    case "cc":
    case "cpp":
    case "hpp":
      return "cpp";
    case "cs":
      return "csharp";
    case "php":
      return "php";
    case "json":
      return "json";
    case "md":
    case "mdx":
      return "markdown";
    case "css":
    case "scss":
    case "sass":
    case "less":
      return "css";
    case "html":
    case "htm":
      return "html";
    case "xml":
      return "xml";
    case "yml":
    case "yaml":
      return "yaml";
    case "toml":
      return "toml";
    case "sh":
    case "bash":
    case "zsh":
    case "ps1":
    case "bat":
    case "cmd":
      return "shell";
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "webp":
    case "svg":
      return "image";
    case "pdf":
      return "pdf";
    case "sql":
    case "sqlite":
    case "db":
      return "database";
    case "txt":
    case "log":
      return "text";
    case "zip":
    case "gz":
    case "tgz":
    case "rar":
    case "7z":
      return "archive";
    case "ttf":
    case "otf":
    case "woff":
    case "woff2":
      return "font";
    default:
      return "default";
  }
}

function TreeIcon({ kind, open = false }: { kind: string; open?: boolean }) {
  if (kind === "folder") {
    return (
      <span
        className={
          open
            ? "tree-icon tree-icon--folder tree-icon--folder-open"
            : "tree-icon tree-icon--folder"
        }
        aria-hidden="true"
        data-icon={open ? "folder-open" : "folder"}
      >
        <svg viewBox="0 0 16 16" focusable="false">
          <path className="tree-icon__folder-tab" d="M1.5 4.5h4.2l1.2 1.4h7.6v1.3h-13z" />
          <path className="tree-icon__folder-body" d="M1.5 6.3h13l-1.2 7H2.3z" />
          {open && <path className="tree-icon__folder-open" d="M2.4 8h12l-1.4 5.3H1z" />}
        </svg>
      </span>
    );
  }

  const shape = fileIconShape(kind);
  return (
    <span
      className={`tree-icon tree-icon--file tree-icon--${kind}`}
      aria-hidden="true"
      data-icon={kind}
    >
      <svg viewBox="0 0 16 16" focusable="false">
        {shape !== "database" && shape !== "terminal" && shape !== "image" && (
          <>
            <path className="tree-icon__page" d="M3 1.7h6.7L13 5v9.3H3z" />
            <path className="tree-icon__fold" d="M9.7 1.7V5H13" />
          </>
        )}
        <FileIconGlyph shape={shape} />
      </svg>
    </span>
  );
}

function FileIconGlyph({ shape }: { shape: string }) {
  switch (shape) {
    case "atom":
      return (
        <>
          <ellipse className="tree-icon__line" cx="8" cy="8.2" rx="5.5" ry="2.1" />
          <ellipse
            className="tree-icon__line"
            cx="8"
            cy="8.2"
            rx="5.5"
            ry="2.1"
            transform="rotate(60 8 8.2)"
          />
          <ellipse
            className="tree-icon__line"
            cx="8"
            cy="8.2"
            rx="5.5"
            ry="2.1"
            transform="rotate(120 8 8.2)"
          />
          <circle className="tree-icon__fill-strong" cx="8" cy="8.2" r="1.2" />
        </>
      );
    case "gear":
      return (
        <>
          <circle className="tree-icon__line" cx="8" cy="8.6" r="3.2" />
          <circle className="tree-icon__fill-strong" cx="8" cy="8.6" r="1.2" />
          <path
            className="tree-icon__line"
            d="M8 4.1v1.4M8 11.7v1.4M4.2 8.6h1.4M10.4 8.6h1.4M5.3 5.9l1 1M9.7 10.2l1 1M10.7 5.9l-1 1M6.3 10.2l-1 1"
          />
        </>
      );
    case "blocks":
      return (
        <>
          <path
            className="tree-icon__fill-soft"
            d="M4.2 5.2h5.1v3H6.1v1.4H3.7V5.7c0-.3.2-.5.5-.5z"
          />
          <path
            className="tree-icon__fill-strong"
            d="M6.7 8.7h5.1v3c0 .3-.2.5-.5.5H6.7v-3H10V7.3h2.3v1.4z"
          />
          <circle className="tree-icon__knockout" cx="5.3" cy="6.2" r=".45" />
          <circle className="tree-icon__knockout" cx="10.7" cy="11.1" r=".45" />
        </>
      );
    case "braces":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M6.2 5.2c-1.2.2-1.4.8-1.4 1.6v.7c0 .5-.3.8-.9.9.6.1.9.4.9.9v.7c0 .9.3 1.4 1.4 1.6M9.8 5.2c1.2.2 1.4.8 1.4 1.6v.7c0 .5.3.8.9.9-.6.1-.9.4-.9.9v.7c0 .9-.3 1.4-1.4 1.6"
        />
      );
    case "markdown":
      return (
        <path
          className="tree-icon__fill-strong"
          d="M4.3 6.1h1.4l1 1.6 1-1.6h1.4v4.5H7.9V8.2l-1.2 1.7-1.2-1.7v2.4H4.3zm5.4 0h1.1v2.2h1.1l-1.7 2.3-1.7-2.3h1.2z"
        />
      );
    case "palette":
      return (
        <>
          <path
            className="tree-icon__line-heavy"
            d="M8 5.1c-2.5 0-4.4 1.6-4.4 3.8 0 1.6 1.1 2.8 2.7 2.8h1.2c.6 0 .8-.3.8-.7s-.3-.6-.3-1 .4-.8 1-.8h.9c1.5 0 2.4-.7 2.4-1.9 0-1.4-1.7-2.2-4.3-2.2z"
          />
          <circle className="tree-icon__fill-strong" cx="5.9" cy="7.5" r=".55" />
          <circle className="tree-icon__fill-strong" cx="8" cy="6.7" r=".55" />
          <circle className="tree-icon__fill-strong" cx="10.1" cy="7.7" r=".55" />
        </>
      );
    case "markup":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M6.3 5.5 4 8l2.3 2.5M9.7 5.5 12 8l-2.3 2.5M8.8 4.9 7.2 11.1"
        />
      );
    case "sliders":
      return (
        <>
          <path className="tree-icon__line" d="M4.5 6h7M4.5 8.6h7M4.5 11.2h7" />
          <circle className="tree-icon__fill-strong" cx="7" cy="6" r=".9" />
          <circle className="tree-icon__fill-strong" cx="10" cy="8.6" r=".9" />
          <circle className="tree-icon__fill-strong" cx="6" cy="11.2" r=".9" />
        </>
      );
    case "terminal":
      return (
        <>
          <rect className="tree-icon__frame" x="2.2" y="3" width="11.6" height="10" rx="1.6" />
          <path className="tree-icon__line-heavy" d="m4.5 6 1.8 1.7-1.8 1.7M7.5 10h3.8" />
        </>
      );
    case "image":
      return (
        <>
          <rect className="tree-icon__frame" x="2.2" y="2.5" width="11.6" height="11" rx="1.4" />
          <circle className="tree-icon__fill-strong" cx="10.7" cy="5.6" r="1" />
          <path className="tree-icon__fill-soft" d="M3.4 12.4 6.4 8.5l2 2.1 1.2-1.4 3 3.2z" />
        </>
      );
    case "pdf":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M4.5 10.7V6.1h1.8c1 0 1.6.6 1.6 1.4s-.6 1.4-1.6 1.4h-.6v1.8M8.8 10.7V6.1h1.1c1.4 0 2.2.9 2.2 2.3s-.8 2.3-2.2 2.3z"
        />
      );
    case "branch":
      return (
        <>
          <circle className="tree-icon__fill-strong" cx="5" cy="5.5" r="1.2" />
          <circle className="tree-icon__fill-strong" cx="11" cy="6.2" r="1.2" />
          <circle className="tree-icon__fill-strong" cx="5" cy="11" r="1.2" />
          <path className="tree-icon__line" d="M5 6.7V9.8M6.1 5.6c2.4 0 2.2.6 3.8.6" />
        </>
      );
    case "flask":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M6.4 4.7h3.2M7.2 4.7v2.8l-2.4 3.8c-.4.7.1 1.5.9 1.5h4.6c.8 0 1.3-.8.9-1.5L8.8 7.5V4.7M5.9 10h4.2"
        />
      );
    case "box":
      return (
        <>
          <path className="tree-icon__fill-soft" d="m8 4 4 2.1-4 2.1-4-2.1z" />
          <path className="tree-icon__fill-strong" d="M4 6.1v4L8 12V8.2zM12 6.1v4L8 12V8.2z" />
        </>
      );
    case "lock":
      return (
        <>
          <rect className="tree-icon__fill-soft" x="4.4" y="7.3" width="7.2" height="5.1" rx="1" />
          <path className="tree-icon__line-heavy" d="M6 7.3V5.8a2 2 0 0 1 4 0v1.5" />
        </>
      );
    case "containers":
      return (
        <>
          <rect className="tree-icon__fill-soft" x="3" y="7" width="3" height="2.4" rx=".4" />
          <rect className="tree-icon__fill-soft" x="6.6" y="7" width="3" height="2.4" rx=".4" />
          <rect className="tree-icon__fill-soft" x="10.2" y="7" width="3" height="2.4" rx=".4" />
          <rect className="tree-icon__fill-strong" x="4.8" y="4.1" width="3" height="2.4" rx=".4" />
          <rect className="tree-icon__fill-strong" x="8.4" y="4.1" width="3" height="2.4" rx=".4" />
          <path className="tree-icon__line" d="M3.3 10.2h9.9" />
        </>
      );
    case "database":
      return (
        <>
          <ellipse className="tree-icon__fill-soft" cx="8" cy="4.2" rx="4.5" ry="1.8" />
          <path
            className="tree-icon__frame"
            d="M3.5 4.2v7.2c0 1 2 1.8 4.5 1.8s4.5-.8 4.5-1.8V4.2"
          />
          <path className="tree-icon__line" d="M3.5 7.8c0 1 2 1.8 4.5 1.8s4.5-.8 4.5-1.8" />
        </>
      );
    case "archive":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M5 5.2h6l1 2v4.6H4V7.2zM5 7.2h6.9M7.1 5.2v6M8.9 5.2v6"
        />
      );
    case "font":
      return (
        <path
          className="tree-icon__line-heavy"
          d="M4.3 11.4 7 4.8h2l2.7 6.6M5.5 8.9h5M11.4 11.4h1.1M3.5 11.4h1.1"
        />
      );
    case "text":
      return <path className="tree-icon__line" d="M5 6.1h6M5 8.2h6M5 10.3h4.5" />;
    default:
      return (
        <path
          className="tree-icon__line-heavy"
          d="M6.5 5.5 4.2 8l2.3 2.5M9.5 5.5 11.8 8l-2.3 2.5"
        />
      );
  }
}

export function FileTreeNode({
  node,
  delta,
  depth,
  activePath,
  expandedDirs,
  onToggleDir,
  onOpen,
  onContextMenu,
  onTreeDragStart,
  onTreeDragEnd,
  onTreeDrop,
  dropTargetPath,
  onPasteInto,
  onDelete,
}: {
  node: TreeNode;
  delta: RepoDelta;
  depth: number;
  activePath: string | null;
  expandedDirs?: Set<string>;
  onToggleDir?: (path: string) => void;
  onOpen: (path: string, pin: boolean) => void;
  onContextMenu?: (event: MouseEvent, node: TreeNode) => void;
  onTreeDragStart?: (node: TreeNode) => void;
  onTreeDragEnd?: () => void;
  onTreeDrop?: (targetPath: string) => void;
  dropTargetPath?: string | null;
  onPasteInto?: (destDirPath: string) => void;
  onDelete?: (node: TreeNode) => void;
}) {
  const [localOpen, setLocalOpen] = useState(false); // fallback for standalone use
  const open = expandedDirs ? expandedDirs.has(node.path) : localOpen;
  const indent = depth * 12 + 8;
  const indentStyle = {
    paddingLeft: `${indent}px`,
    "--indent": `${indent}px`,
  } as CSSProperties;
  const toggleDir = () => {
    if (onToggleDir) onToggleDir(node.path);
    else setLocalOpen((o) => !o);
  };

  if (node.isDir) {
    const dirClassBase = node.hasChanges ? "tree-dir__row tree-dir__row--changed" : "tree-dir__row";
    const isDropTarget = dropTargetPath === node.path;
    const dirClass = isDropTarget
      ? `${dirClassBase} tree-dir__row--drop-target tree-dir__row--drop-target-hover`
      : dirClassBase;
    return (
      <div className="tree-dir">
        <button
          className={dirClass}
          type="button"
          style={indentStyle}
          draggable={!!onTreeDragStart}
          onDragStart={() => onTreeDragStart?.(node)}
          onDragEnd={() => onTreeDragEnd?.()}
          onDragOver={(event) => {
            if (onTreeDrop) {
              event.preventDefault();
            }
          }}
          onDrop={(event) => {
            if (onTreeDrop) {
              event.preventDefault();
              event.stopPropagation();
              onTreeDrop(node.path);
            }
          }}
          onPaste={(event) => {
            // onPaste del webview: los archivos del clipboard del navegador no
            // traen paths del SO, así que delegamos al clipboard interno
            // (Ctrl+C dentro del árbol) en lugar de usarlo aquí.
            // (Ctrl+V en la carpeta se maneja vía keydown abajo.)
            void event;
          }}
          onClick={toggleDir}
          onContextMenu={(event) => onContextMenu?.(event, node)}
          onKeyDown={(event) => {
            if (event.key === "Delete") {
              event.preventDefault();
              event.stopPropagation();
              onDelete?.(node);
            }
          }}
        >
          <span
            className={`tree-dir__caret${open ? " tree-dir__caret--open" : ""}`}
            aria-hidden="true"
          />
          <TreeIcon kind="folder" open={open} />
          <span className="tree-dir__name" title={node.name}>
            {node.name}
          </span>
          {node.hasChanges && (
            <span
              className="tree-dir__dot"
              data-testid={`tree-dir-changed-${node.path}`}
              title="Contiene archivos con cambios"
            >
              ●
            </span>
          )}
        </button>
        {open &&
          node.children.map((c) => (
            <FileTreeNode
              key={c.path}
              node={c}
              delta={delta}
              depth={depth + 1}
              activePath={activePath}
              expandedDirs={expandedDirs}
              onToggleDir={onToggleDir}
              onOpen={onOpen}
              onContextMenu={onContextMenu}
              onTreeDragStart={onTreeDragStart}
              onTreeDragEnd={onTreeDragEnd}
              onTreeDrop={onTreeDrop}
              dropTargetPath={dropTargetPath}
              onPasteInto={onPasteInto}
              onDelete={onDelete}
            />
          ))}
      </div>
    );
  }

  const classes = ["tree-file"];
  if (node.changed) classes.push("tree-file--changed");
  if (node.path === activePath) classes.push("tree-file--active");
  const iconKind = fileIconKind(node.name);

  return (
    <div
      className={classes.join(" ")}
      style={indentStyle}
      role="button"
      tabIndex={0}
      data-testid={`tree-file-${node.path}`}
      draggable={!!onTreeDragStart}
      onDragStart={() => onTreeDragStart?.(node)}
      onDragEnd={() => onTreeDragEnd?.()}
      onClick={() => onOpen(node.path, false)}
      onDoubleClick={() => onOpen(node.path, true)}
      onContextMenu={(event) => onContextMenu?.(event, node)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen(node.path, true);
        } else if (e.key === " ") {
          e.preventDefault();
          onOpen(node.path, false);
        } else if (e.key === "Delete") {
          e.preventDefault();
          e.stopPropagation();
          onDelete?.(node);
        }
      }}
    >
      <TreeIcon kind={iconKind} />
      <span className="tree-file__name" title={node.name}>
        {node.name}
      </span>
      {node.changed && (
        <span className={`tree-file__mark tree-file__mark--${node.changed}`}>
          {MARK[node.changed]}
        </span>
      )}
      <SignalBadges signals={getPathSignals(delta, node.path)} limit={1} compact />
    </div>
  );
}
