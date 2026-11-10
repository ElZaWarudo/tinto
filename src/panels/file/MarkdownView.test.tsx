import { describe, expect, it, vi } from "vitest";
import { createEvent, fireEvent, render, screen } from "@testing-library/react";

const invokeMock = vi.fn<(command: string, args?: unknown) => Promise<void>>(() =>
  Promise.resolve(),
);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invokeMock(command, args),
}));

import { MarkdownLink } from "./MarkdownView";

describe("MarkdownLink", () => {
  it.each([
    ["https://example.com/docs", true, true],
    ["docs/other.md", true, false],
    ["#section", false, false],
  ])("%s: keeps the app window and opens only web addresses", (href, prevented, opened) => {
    invokeMock.mockClear();
    render(<MarkdownLink href={href}>enlace</MarkdownLink>);
    const link = screen.getByText("enlace");
    const click = createEvent.click(link);
    fireEvent(link, click);

    expect(click.defaultPrevented).toBe(prevented);
    if (opened) expect(invokeMock).toHaveBeenCalledWith("delivery_open_url", { url: href });
    else expect(invokeMock).not.toHaveBeenCalled();
  });
});
