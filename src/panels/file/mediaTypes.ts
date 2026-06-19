export type MediaKind = "image" | "pdf";

export const IMAGE_MIME_BY_EXT: Record<string, string> = {
  avif: "image/avif",
  bmp: "image/bmp",
  gif: "image/gif",
  ico: "image/x-icon",
  jpeg: "image/jpeg",
  jpg: "image/jpeg",
  png: "image/png",
  svg: "image/svg+xml",
  webp: "image/webp",
};

function extension(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot < 0 ? "" : path.slice(dot + 1).toLowerCase();
}

export function mediaKind(path: string): MediaKind | null {
  const ext = extension(path);
  if (ext === "pdf") return "pdf";
  if (IMAGE_MIME_BY_EXT[ext]) return "image";
  return null;
}

export function mimeFor(path: string, kind: MediaKind): string {
  if (kind === "pdf") return "application/pdf";
  return IMAGE_MIME_BY_EXT[extension(path)] ?? "application/octet-stream";
}
