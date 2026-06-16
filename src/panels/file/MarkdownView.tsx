// Rendered Markdown view: fetches a file's current working-tree content and
// renders it formatted (GFM: tables, task lists, strikethrough). react-markdown
// builds a React tree (no innerHTML), so untrusted repo content can't inject
// markup. Mirrors FullFileView's fetch/degrade states; the source toggle is
// owned by the parent FileView (which swaps in FullFileView for raw source).

import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { getFileContent } from "../../bus/client";
import type { FileContent } from "../../bus/contract";

export function MarkdownView({ repo, path }: { repo: string; path: string }) {
  const [content, setContent] = useState<FileContent | undefined>(undefined);
  const [error, setError] = useState(false);

  useEffect(() => {
    let active = true;
    getFileContent(repo, path)
      .then((c) => {
        if (active) {
          setContent(c);
          setError(false);
        }
      })
      .catch(() => active && setError(true));
    return () => {
      active = false;
    };
  }, [repo, path]);

  if (error) {
    return (
      <div className="markdown-view markdown-view--error" data-testid="md-error">
        Could not load file.
      </div>
    );
  }
  if (content === undefined) {
    return (
      <div className="markdown-view markdown-view--loading" data-testid="md-loading">
        Loading…
      </div>
    );
  }
  if (content.encoding === "base64") {
    return (
      <div className="markdown-view markdown-view--binary" data-testid="md-binary">
        Binary file — cannot render.
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
          File truncated at the read limit — content beyond this point is not shown.
        </div>
      )}
    </div>
  );
}
