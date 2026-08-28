import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AgentSession } from "../../bus/contract";
import { AgentTreeLens } from "./AgentTreeLens";

const fixture = (): AgentSession => ({
  id: "root",
  repo: "/repo",
  agent_type: "codex",
  permission_mode: "workspace",
  permission_mode_change_supported: false,
  status: "running",
  pid: 1,
  started_at_ms: 1,
  ended_at_ms: null,
  exit_code: null,
  error: null,
  turn_status: "waiting",
  turn_interrupt_supported: true,
  active_sessions: 1,
  age_ms: 1,
  subagents: [
    {
      id: "child",
      parent_id: "root",
      source_kind: "subAgent",
      depth: 1,
      nickname: "Scout",
      role: "researcher",
      model: "gpt-5.6-sol",
      reasoning_effort: "high",
      thread_status: "running",
      turn_status: "working",
      capabilities: {
        inspect: true,
        direct_input: true,
        steer: true,
        interrupt: true,
        wait: true,
        close: true,
      },
      activities: [
        { id: "a1", kind: "interacted", status: "inProgress", text: "Searching", timestamp_ms: 2 },
      ],
      result: null,
      timeline: [
        {
          session_id: "child",
          id: "m1",
          kind: "agent_message",
          text: "Child only",
          timestamp_ms: 2,
        },
      ],
      updated_at_ms: 2,
    },
  ],
});

describe("AgentTreeLens", () => {
  it("renders hierarchy, child detail and only the selected transcript", async () => {
    const user = userEvent.setup();
    render(<AgentTreeLens session={fixture()} readOnly={false} />);
    const tree = screen.getByRole("tree", { name: "Agent hierarchy" });
    expect(within(tree).getByText("codex (primary)")).toBeInTheDocument();
    await user.click(within(tree).getByText("Scout"));
    expect(screen.getByText("Provider ID")).toBeInTheDocument();
    expect(screen.getByText("Child only")).toBeInTheDocument();
    expect(screen.getByLabelText("Current activity: Searching")).toBeInTheDocument();
  });

  it("supports tree keyboard navigation and truthful controls", async () => {
    const user = userEvent.setup();
    const onDirectInput = vi.fn(() => Promise.resolve());
    const onInterrupt = vi.fn(() => Promise.resolve());
    render(
      <AgentTreeLens
        session={fixture()}
        readOnly={false}
        onDirectInput={onDirectInput}
        onInterrupt={onInterrupt}
      />,
    );
    const root = screen.getByRole("treeitem", { name: /codex \(primary\)/ });
    root.focus();
    await user.keyboard("{ArrowDown}");
    const child = screen.getByRole("treeitem", { name: /Scout/ });
    expect(child).toHaveFocus();
    const message = screen.getByPlaceholderText("Send a follow-up or steer...");
    await user.type(message, "Check this");
    await user.click(screen.getByRole("button", { name: "Follow-up" }));
    expect(onDirectInput).toHaveBeenCalledWith("child", "Check this");
    expect(screen.getByRole("button", { name: "Steer" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Request close" })).toBeDisabled();
    fireEvent.keyDown(child, { key: "Home" });
    expect(root).toHaveFocus();
  });

  it("keeps restored history inspectable but disables mutations", () => {
    render(<AgentTreeLens session={fixture()} readOnly />);
    const tree = screen.getByRole("tree");
    fireEvent.click(within(tree).getByText("Scout"));
    expect(screen.getByRole("button", { name: "Follow-up" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Follow-up" })).toHaveAttribute(
      "title",
      "Restored history is read-only.",
    );
    expect(screen.getByText("Child only")).toBeInTheDocument();
  });

  it("hides an entire collapsed subtree and restores focus to the ancestor", async () => {
    const nested = fixture();
    nested.subagents?.push({
      id: "grandchild",
      parent_id: "child",
      source_kind: "subAgent",
      depth: 2,
      nickname: "Verifier",
      thread_status: "running",
      turn_status: "working",
      capabilities: {
        inspect: true,
        direct_input: false,
        steer: false,
        interrupt: false,
        wait: false,
        close: false,
      },
      timeline: [],
      updated_at_ms: 3,
    });
    const user = userEvent.setup();
    render(<AgentTreeLens session={nested} readOnly={false} />);
    await user.click(screen.getByRole("button", { name: "Expand Scout" }));
    const grandchild = screen.getByRole("treeitem", { name: /Verifier/ });
    grandchild.focus();
    await user.click(screen.getByRole("button", { name: /Collapse codex \(primary\)/ }));
    expect(screen.queryByRole("treeitem", { name: /Verifier/ })).not.toBeInTheDocument();
    expect(screen.getByRole("treeitem", { name: /codex \(primary\)/ })).toHaveFocus();
  });

  it("preserves visual indentation for arbitrary nesting depth", async () => {
    const nested = fixture();
    for (let depth = 2; depth <= 6; depth += 1) {
      nested.subagents?.push({
        id: `nested-${depth}`,
        parent_id: depth === 2 ? "child" : `nested-${depth - 1}`,
        source_kind: "subAgent",
        depth,
        nickname: `Depth ${depth}`,
        thread_status: "running",
        turn_status: "working",
        capabilities: {
          inspect: true,
          direct_input: false,
          steer: false,
          interrupt: false,
          wait: false,
          close: false,
        },
        timeline: [],
        updated_at_ms: depth + 1,
      });
    }
    const user = userEvent.setup();
    render(<AgentTreeLens session={nested} readOnly={false} />);
    await user.click(screen.getByRole("button", { name: "Expand Scout" }));
    for (let depth = 2; depth < 6; depth += 1) {
      await user.click(screen.getByRole("button", { name: `Expand Depth ${depth}` }));
    }
    const deepest = screen.getByRole("treeitem", { name: /Depth 6/ });
    expect(deepest).toHaveAttribute("aria-level", "7");
    expect(deepest).toHaveStyle("--agent-tree-level: 7");
  });

  it("keeps drafts scoped to one agent and describes close as a request", async () => {
    const multi = fixture();
    multi.subagents?.push({ ...multi.subagents![0], id: "child-2", nickname: "Builder" });
    const onClose = vi.fn(() => Promise.resolve());
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();
    render(<AgentTreeLens session={multi} readOnly={false} onClose={onClose} />);
    await user.click(screen.getByText("Scout"));
    await user.type(screen.getByPlaceholderText("Send a follow-up or steer..."), "Scout only");
    await user.click(screen.getByText("Builder"));
    expect(screen.getByPlaceholderText("Send a follow-up or steer...")).toHaveValue("");
    await user.click(screen.getByRole("button", { name: "Request close" }));
    expect(onClose).toHaveBeenCalledWith("child-2");
    expect(screen.getByRole("status")).toHaveTextContent("Waiting for Codex status confirmation");
    confirm.mockRestore();
  });
});
