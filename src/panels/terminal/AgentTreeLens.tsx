import { useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent } from "react";
import type { AgentSession, AgentSubagentCapabilities } from "../../bus/contract";
import {
  agentCapabilityReason,
  agentStatusLabel,
  selectAgentTree,
  selectLatestAgentActivity,
  type AgentTreeNode,
} from "../../agent/subagentSelectors";

export interface AgentTreeLensProps {
  session: AgentSession;
  readOnly: boolean;
  onDirectInput?: (threadId: string, text: string) => Promise<unknown>;
  onSteer?: (threadId: string, text: string) => Promise<unknown>;
  onInterrupt?: (threadId: string) => Promise<unknown>;
  onWait?: (threadId: string) => Promise<unknown>;
  onClose?: (threadId: string) => Promise<unknown>;
}

type PendingAction = `${string}:${keyof AgentSubagentCapabilities}` | null;

const CONTROL_LABELS: Array<{
  capability: keyof AgentSubagentCapabilities;
  label: string;
}> = [
  { capability: "direct_input", label: "Follow-up" },
  { capability: "steer", label: "Steer" },
  { capability: "interrupt", label: "Interrupt" },
  { capability: "wait", label: "Wait" },
  { capability: "close", label: "Request close" },
];

function nodeLabel(node: AgentTreeNode): string {
  return node.kind === "root" ? `${node.label} (primary)` : node.label;
}

export function AgentTreeLens({
  session,
  readOnly,
  onDirectInput,
  onSteer,
  onInterrupt,
  onWait,
  onClose,
}: AgentTreeLensProps) {
  const nodes = useMemo(() => selectAgentTree(session), [session]);
  const [selectedId, setSelectedId] = useState(session.id);
  const [focusedId, setFocusedId] = useState(session.id);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([session.id]));
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [pending, setPending] = useState<PendingAction>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const nodeRefs = useRef(new Map<string, HTMLDivElement>());

  useEffect(() => {
    if (nodes.some((node) => node.id === focusedId)) return;
    const frame = requestAnimationFrame(() => {
      setFocusedId(session.id);
      nodeRefs.current.get(session.id)?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [focusedId, nodes, session.id]);

  const visibleNodes = useMemo(() => {
    const visible: AgentTreeNode[] = [];
    const byId = new Map(nodes.map((node) => [node.id, node]));
    const isVisible = (node: AgentTreeNode) => {
      let parentId = node.parentId;
      const seen = new Set<string>();
      while (parentId) {
        if (seen.has(parentId) || !expanded.has(parentId)) return false;
        seen.add(parentId);
        parentId = byId.get(parentId)?.parentId ?? null;
      }
      return true;
    };
    for (const node of nodes) {
      if (node.id === session.id) {
        visible.push(node);
        continue;
      }
      if (!isVisible(node)) continue;
      visible.push(node);
    }
    return visible;
  }, [expanded, nodes, session.id]);
  const selected = nodes.find((node) => node.id === selectedId) ?? nodes[0];
  const draft = selected ? (drafts[selected.id] ?? "") : "";
  const selectedIndex = Math.max(
    0,
    visibleNodes.findIndex((node) => node.id === focusedId),
  );
  const childCount = Math.max(0, nodes.length - 1);

  const focusNode = (node: AgentTreeNode) => {
    setFocusedId(node.id);
    nodeRefs.current.get(node.id)?.focus();
    requestAnimationFrame(() => nodeRefs.current.get(node.id)?.focus());
  };

  const selectNode = (node: AgentTreeNode) => {
    setSelectedId(node.id);
    setFocusedId(node.id);
  };

  const toggleNode = (node: AgentTreeNode) => {
    if (node.kind === "root" || nodes.some((candidate) => candidate.parentId === node.id)) {
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(node.id)) {
          next.delete(node.id);
          const byId = new Map(nodes.map((candidate) => [candidate.id, candidate]));
          let currentId: string | null = focusedId;
          while (currentId) {
            if (currentId === node.id) {
              setSelectedId(node.id);
              focusNode(node);
              break;
            }
            currentId = byId.get(currentId)?.parentId ?? null;
          }
        } else next.add(node.id);
        return next;
      });
    }
  };

  const handleTreeKeyDown = (event: KeyboardEvent<HTMLDivElement>, node: AgentTreeNode) => {
    const childNodes = nodes.filter((candidate) => candidate.parentId === node.id);
    if (
      event.key === "ArrowDown" ||
      event.key === "ArrowUp" ||
      event.key === "Home" ||
      event.key === "End"
    ) {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
      const index =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? visibleNodes.length - 1
            : selectedIndex + delta;
      const next = visibleNodes[Math.max(0, Math.min(visibleNodes.length - 1, index))];
      if (next) {
        selectNode(next);
        focusNode(next);
      }
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      if (childNodes.length > 0 && !expanded.has(node.id)) toggleNode(node);
      else if (childNodes[0]) focusNode(childNodes[0]);
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      if (expanded.has(node.id) && childNodes.length > 0) toggleNode(node);
      else {
        const parent = nodes.find((candidate) => candidate.id === node.parentId);
        if (parent) focusNode(parent);
      }
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectNode(node);
    }
  };

  const dispatch = async (capability: keyof AgentSubagentCapabilities) => {
    if (!selected || selected.kind === "root") return;
    const reason = agentCapabilityReason(selected, capability, readOnly);
    if (reason) {
      setNotice(reason);
      return;
    }
    const action = `${selected.id}:${capability}` as PendingAction;
    setPending(action);
    setNotice(null);
    try {
      if (capability === "direct_input" || capability === "steer") {
        const text = draft.trim();
        if (!text) {
          setNotice("Write a message before sending it to the selected agent.");
          return;
        }
        const handler = capability === "direct_input" ? onDirectInput : onSteer;
        if (!handler) throw new Error("This control is not connected.");
        await handler(selected.providerId, text);
        setDrafts((current) => ({ ...current, [selected.id]: "" }));
      } else if (capability === "interrupt") {
        if (!onInterrupt) throw new Error("This control is not connected.");
        await onInterrupt(selected.providerId);
      } else if (capability === "wait") {
        if (!onWait) throw new Error("This control is not connected.");
        await onWait(selected.providerId);
      } else if (capability === "close") {
        if (!onClose) throw new Error("This control is not connected.");
        if (!window.confirm(`Request Codex to close ${nodeLabel(selected)}?`)) return;
        await onClose(selected.providerId);
      }
      setNotice(
        capability === "close"
          ? `Close requested for ${nodeLabel(selected)}. Waiting for Codex status confirmation.`
          : `${CONTROL_LABELS.find((item) => item.capability === capability)?.label ?? capability} sent for ${nodeLabel(selected)}.`,
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : "The agent action could not be sent.");
    } finally {
      setPending(null);
    }
  };

  return (
    <section className="agent-panel__subagents" aria-label="Agent hierarchy">
      <div className="agent-panel__subagents-head">
        <div>
          <strong>Agents</strong>
          <small>
            {childCount} descendant{childCount === 1 ? "" : "s"}
          </small>
        </div>
        <span aria-live="polite" role="status" title="Agent lifecycle updates">
          {notice ??
            (selected
              ? `${nodeLabel(selected)}: ${agentStatusLabel(selected.status)}. ${selectLatestAgentActivity(selected)?.text ?? "No activity reported."}`
              : "")}
        </span>
      </div>
      <div className="agent-panel__subagents-tree" role="tree" aria-label="Agent hierarchy">
        {visibleNodes.map((node) => {
          const hasChildren = nodes.some((candidate) => candidate.parentId === node.id);
          const isExpanded = expanded.has(node.id);
          return (
            <div
              aria-expanded={hasChildren ? isExpanded : undefined}
              aria-level={node.depth}
              aria-selected={node.id === selected?.id}
              className={`agent-panel__subagent-treeitem${node.id === selected?.id ? " agent-panel__subagent-treeitem--selected" : ""}`}
              key={node.id}
              onClick={() => selectNode(node)}
              onKeyDown={(event) => handleTreeKeyDown(event, node)}
              ref={(element) => {
                if (element) nodeRefs.current.set(node.id, element);
                else nodeRefs.current.delete(node.id);
              }}
              role="treeitem"
              style={{ "--agent-tree-level": node.depth } as CSSProperties}
              tabIndex={node.id === focusedId ? 0 : -1}
              title={`${nodeLabel(node)}: ${agentStatusLabel(node.status)}`}
            >
              <button
                aria-label={`${isExpanded ? "Collapse" : "Expand"} ${nodeLabel(node)}`}
                className="agent-panel__subagent-toggle"
                disabled={!hasChildren}
                onClick={(event) => {
                  event.stopPropagation();
                  toggleNode(node);
                }}
                tabIndex={-1}
                type="button"
              >
                {hasChildren ? (isExpanded ? "▾" : "▸") : "·"}
              </button>
              <span className="agent-panel__subagent-name">{nodeLabel(node)}</span>
              <span
                className={`agent-panel__subagent-status agent-panel__subagent-status--${node.status}`}
              >
                {agentStatusLabel(node.status)}
              </span>
            </div>
          );
        })}
      </div>
      {selected && (
        <AgentDetail
          node={selected}
          readOnly={readOnly}
          pending={pending}
          draft={draft}
          setDraft={(value) => setDrafts((current) => ({ ...current, [selected.id]: value }))}
          dispatch={dispatch}
          connected={{
            direct_input: Boolean(onDirectInput),
            steer: Boolean(onSteer),
            interrupt: Boolean(onInterrupt),
            wait: Boolean(onWait),
            close: Boolean(onClose),
          }}
        />
      )}
    </section>
  );
}

function AgentDetail({
  node,
  readOnly,
  pending,
  draft,
  setDraft,
  dispatch,
  connected,
}: {
  node: AgentTreeNode;
  readOnly: boolean;
  pending: PendingAction;
  draft: string;
  setDraft: (value: string) => void;
  dispatch: (capability: keyof AgentSubagentCapabilities) => Promise<void>;
  connected: Partial<Record<keyof AgentSubagentCapabilities, boolean>>;
}) {
  const latest = selectLatestAgentActivity(node);
  return (
    <div className="agent-panel__subagent-detail" aria-label={`Details for ${nodeLabel(node)}`}>
      <div className="agent-panel__subagent-detail-head">
        <strong>{nodeLabel(node)}</strong>
        <span>
          {agentStatusLabel(node.status)} · {agentStatusLabel(node.turnStatus)}
        </span>
      </div>
      <dl className="agent-panel__subagent-meta">
        <div>
          <dt>Role</dt>
          <dd>{node.role ?? "Inherited"}</dd>
        </div>
        <div>
          <dt>Provider ID</dt>
          <dd>{node.providerId}</dd>
        </div>
        <div>
          <dt>Model</dt>
          <dd>{node.model ?? "Inherited"}</dd>
        </div>
        <div>
          <dt>Reasoning</dt>
          <dd>{node.reasoningEffort ?? "Inherited"}</dd>
        </div>
        <div>
          <dt>Runtime</dt>
          <dd>{node.runtime ?? "Inherited"}</dd>
        </div>
        <div>
          <dt>Permission</dt>
          <dd>{node.permissionMode ?? "Inherited"}</dd>
        </div>
      </dl>
      <p
        aria-label={`Current activity: ${latest?.text ?? "No activity reported."}`}
        className="agent-panel__subagent-activity"
      >
        <b>Activity</b>: {latest?.text ?? "No activity reported."}
      </p>
      {node.result && (
        <div className="agent-panel__subagent-result" aria-label="Agent result">
          <b>Result · {agentStatusLabel(node.result.status)}</b>
          <p>{node.result.summary ?? node.result.error ?? "No result summary."}</p>
        </div>
      )}
      <div
        className="agent-panel__subagent-transcript"
        aria-label={`Transcript for ${nodeLabel(node)}`}
      >
        <b>Transcript</b>
        {node.timeline.length > 0 ? (
          node.timeline.map((item) => (
            <article key={item.id}>
              <small>
                {item.kind} · {new Date(item.timestamp_ms).toLocaleTimeString()}
              </small>
              <p>{item.text}</p>
            </article>
          ))
        ) : (
          <p className="agent-panel__empty-lens">No transcript captured.</p>
        )}
      </div>
      {node.kind !== "root" && (
        <div
          className="agent-panel__subagent-controls"
          aria-label={`Controls for ${nodeLabel(node)}`}
        >
          <label>
            Message
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.currentTarget.value)}
              placeholder="Send a follow-up or steer..."
            />
          </label>
          {CONTROL_LABELS.map(({ capability, label }) => {
            const reason = agentCapabilityReason(node, capability, readOnly);
            const isConnected = connected[capability] === true;
            const reasonId = `agent-${node.id.replace(/[^a-zA-Z0-9_-]/g, "-")}-${capability}-reason`;
            return (
              <span className="agent-panel__subagent-control" key={capability}>
                <button
                  aria-describedby={reason || !isConnected ? reasonId : undefined}
                  disabled={Boolean(reason) || !isConnected || pending !== null}
                  onClick={() => void dispatch(capability)}
                  title={reason ?? `Send ${label} to ${nodeLabel(node)}`}
                  type="button"
                >
                  {pending === `${node.id}:${capability}` ? `${label}…` : label}
                </button>
                {(reason || !isConnected) && (
                  <span className="sr-only" id={reasonId}>
                    {reason ?? "This control is not connected."}
                  </span>
                )}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
