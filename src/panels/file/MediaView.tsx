import { useEffect, useMemo, useState } from "react";
import { getMediaContent } from "../../bus/client";
import type { FileContent } from "../../bus/contract";
import { mimeFor, type MediaKind } from "./mediaTypes";

function dataUrl(content: FileContent, mime: string): string | null {
  if (content.encoding !== "base64" || content.truncated) return null;
  return `data:${mime};base64,${content.content}`;
}

export function MediaView({ repo, path, kind }: { repo: string; path: string; kind: MediaKind }) {
  const [loaded, setLoaded] = useState<
    { repo: string; path: string; content: FileContent } | undefined
  >(undefined);
  const [failed, setFailed] = useState<{ repo: string; path: string } | undefined>(undefined);
  const [requestVersion, setRequestVersion] = useState(0);
  const mime = mimeFor(path, kind);
  const content = loaded?.repo === repo && loaded.path === path ? loaded.content : undefined;
  const error = failed?.repo === repo && failed.path === path;
  const src = useMemo(() => (content ? dataUrl(content, mime) : null), [content, mime]);

  useEffect(() => {
    let active = true;
    getMediaContent(repo, path)
      .then((c) => {
        if (active) {
          setLoaded({ repo, path, content: c });
          setFailed(undefined);
        }
      })
      .catch(() => active && setFailed({ repo, path }));
    return () => {
      active = false;
    };
  }, [repo, path, requestVersion]);

  if (error) {
    return (
      <div className="media-view media-view--state" data-testid="media-error" role="alert">
        <span>No se pudo cargar la vista previa.</span>
        <button
          type="button"
          onClick={() => {
            setLoaded(undefined);
            setFailed(undefined);
            setRequestVersion((version) => version + 1);
          }}
        >
          Reintentar
        </button>
      </div>
    );
  }
  if (content === undefined) {
    return (
      <div className="media-view media-view--state" data-testid="media-loading" role="status">
        Cargando vista previa…
      </div>
    );
  }
  if (!src) {
    return (
      <div className="media-view media-view--state" data-testid="media-unavailable">
        {content.truncated
          ? "La vista previa no está disponible porque el archivo supera el límite de lectura."
          : "La vista previa no está disponible para esta codificación."}
      </div>
    );
  }

  if (kind === "pdf") {
    return (
      <div className="media-view media-view--pdf" data-testid="pdf-view">
        <iframe className="media-view__pdf" title={path} src={src} />
      </div>
    );
  }

  return (
    <div className="media-view media-view--image" data-testid="image-view">
      <img className="media-view__image" src={src} alt={path} />
    </div>
  );
}
