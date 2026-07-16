import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AgentConversationTab } from "./AgentConversationTab";

describe("AgentConversationTab", () => {
  it("shows agent and project above the first-message title", () => {
    let onTitleChange: ((event: { title: string }) => void) | undefined;
    const api = {
      title: "Codex · tinto · Corrige la carga inicial del dashboard",
      close: vi.fn(),
      onDidTitleChange: vi.fn((listener: (event: { title: string }) => void) => {
        onTitleChange = listener;
        return { dispose: vi.fn() };
      }),
    };

    render(
      <AgentConversationTab
        api={api as never}
        containerApi={{} as never}
        params={{}}
        tabLocation="header"
      />,
    );

    expect(screen.getByText("Codex · tinto")).toBeInTheDocument();
    expect(screen.getByText("Corrige la carga inicial del dashboard")).toBeInTheDocument();

    act(() => onTitleChange?.({ title: "Codex · tinto · Revisa el chat" }));
    expect(screen.getByText("Revisa el chat")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cerrar Revisa el chat" }));
    expect(api.close).toHaveBeenCalledOnce();
  });
});
