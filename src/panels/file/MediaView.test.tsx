import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { FileContent } from "../../bus/contract";

let content: FileContent = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
let reject = false;
let pending: Promise<FileContent> | null = null;

vi.mock("../../bus/client", () => ({
  getMediaContent: vi.fn(() => {
    if (pending) return pending;
    if (reject) return Promise.reject(new Error("boom"));
    return Promise.resolve(content);
  }),
}));

import { MediaView } from "./MediaView";
import { mediaKind } from "./mediaTypes";

describe("mediaKind", () => {
  it("detects supported images and PDFs by extension", () => {
    expect(mediaKind("docs/spec.PDF")).toBe("pdf");
    expect(mediaKind("assets/logo.png")).toBe("image");
    expect(mediaKind("assets/icon.svg")).toBe("image");
    expect(mediaKind("src/main.ts")).toBeNull();
  });
});

describe("MediaView", () => {
  beforeEach(() => {
    content = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
    reject = false;
    pending = null;
  });

  it("renders an image data URL from base64 media content", async () => {
    render(<MediaView repo="/r/a" path="brand/logo.png" kind="image" />);

    const img = await screen.findByRole("img");
    expect(img).toHaveAttribute("src", "data:image/png;base64,iVBORw0KGgo=");
    expect(img).toHaveAttribute("alt", "brand/logo.png");
  });

  it("renders a PDF iframe from base64 media content", async () => {
    content = { encoding: "base64", content: "JVBERi0x", truncated: false };
    render(<MediaView repo="/r/a" path="docs/spec.pdf" kind="pdf" />);

    const frame = await screen.findByTitle("docs/spec.pdf");
    expect(frame).toHaveAttribute("src", "data:application/pdf;base64,JVBERi0x");
  });

  it("does not render truncated media bytes", async () => {
    content = { encoding: "base64", content: "partial", truncated: true };
    render(<MediaView repo="/r/a" path="large.pdf" kind="pdf" />);

    expect(await screen.findByTestId("media-unavailable")).toHaveTextContent(
      "supera el límite de lectura",
    );
  });

  it("does not keep showing stale media while a new path loads", async () => {
    const { rerender } = render(<MediaView repo="/r/a" path="brand/logo.png" kind="image" />);
    expect(await screen.findByRole("img")).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgo=",
    );

    let resolveNext: (value: FileContent) => void = () => {};
    pending = new Promise((resolve) => {
      resolveNext = resolve;
    });
    rerender(<MediaView repo="/r/a" path="brand/other.png" kind="image" />);

    expect(screen.getByTestId("media-loading")).toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();

    pending = null;
    await act(async () => {
      resolveNext({ encoding: "base64", content: "bmV3", truncated: false });
    });
    expect(await screen.findByRole("img")).toHaveAttribute("src", "data:image/png;base64,bmV3");
  });

  it("shows an error state and can retry media loading", async () => {
    reject = true;
    render(<MediaView repo="/r/a" path="missing.png" kind="image" />);

    expect(await screen.findByTestId("media-error")).toHaveTextContent(
      "No se pudo cargar la vista previa.",
    );

    reject = false;
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    expect(await screen.findByRole("img")).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgo=",
    );
  });
});
