// Rendered Markdown view: fetches a file's current working-tree content and
// renders it formatted (GFM: tables, task lists, strikethrough). react-markdown
// builds a React tree (no innerHTML), so untrusted repo content can't inject
// markup. Mirrors FullFileView's fetch/degrade states; the source toggle is
// owned by the parent FileView (which swaps in FullFileView for raw source).

import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { FileContent } from "../../bus/contract";
import { fileLoadErrorMessage, loadFileContentWithRetry } from "./fileContentLoader";

interface LoadedFileContent {
  key: string;
  content: FileContent;
}

export function MarkdownView({ repo, path }: { repo: string; path: string }) {
  const [loaded, setLoaded] = useState<LoadedFileContent | null>(null);
  const [error, setError] = useState<{ key: string; message: string } | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const requestKey = `${repo}\0${path}\0${reloadToken}`;
  const content = loaded?.key === requestKey ? loaded.content : undefined;
  const errorMessage = error?.key === requestKey ? error.message : null;

  useEffect(() => {
    let active = true;
    const key = requestKey;
    loadFileContentWithRetry(repo, path)
      .then((c) => {
        if (active) {
          setLoaded({ key, content: c });
          setError(null);
        }
      })
      .catch((cause) => active && setError({ key, message: fileLoadErrorMessage(cause) }));
    return () => {
      active = false;
    };
  }, [repo, path, reloadToken, requestKey]);

  if (errorMessage) {
    return (
      <div className="markdown-view markdown-view--error" data-testid="md-error" role="alert">
        <span>No se pudo cargar el archivo: {errorMessage}</span>
        <button type="button" onClick={() => setReloadToken((token) => token + 1)}>
          Reintentar
        </button>
      </div>
    );
  }
  if (content === undefined) {
    return (
      <div className="markdown-view markdown-view--loading" data-testid="md-loading" role="status">
        Cargando…
      </div>
    );
  }
  if (content.encoding === "base64") {
    return (
      <div className="markdown-view markdown-view--binary" data-testid="md-binary">
        Archivo binario: no se puede renderizar.
      </div>
    );
  }

  return (
    <div className="markdown-view" data-testid="markdown-view">
      <div className="markdown-view__body">
        <Markdown remarkPlugins={[remarkGfm]}>{content.content}</Markdown>
      </div>
      {content.truncated && (
        <div className="diff-view__notice" data-testid="md-truncated">
          Archivo truncado en el límite de lectura; el contenido restante no se muestra.
        </div>
      )}
    </div>
  );
}
