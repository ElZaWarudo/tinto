import type {
  AgentSession,
  AgentSubagentActivity,
  AgentSubagentCapabilities,
  AgentSubagentResult,
  AgentSubagentThread,
  AgentSessionTimelineItem,
} from "../bus/contract";

const MAX_AGENT_TIMELINE_ITEMS = 2000;
const MAX_AGENT_ACTIVITY_ITEMS = 2000;

export interface AgentTreeNode {
  id: string;
  providerId: string;
  kind: "root" | "subagent";
  parentId: string | null;
  sourceParentId: string | null;
  depth: number;
  label: string;
  role: string | null;
  status: string;
  turnStatus: string;
  activity: AgentSubagentActivity[];
  result: AgentSubagentResult | null;
  timeline: AgentSessionTimelineItem[];
  capabilities: AgentSubagentCapabilities;
  model: string | null;
  reasoningEffort: string | null;
  runtime: string | null;
  approvalPolicy: string | null;
  permissionMode: string | null;
  capacity: number | null;
  agentPath: string[];
}

const NO_CAPABILITIES: AgentSubagentCapabilities = {
  inspect: true,
  direct_input: false,
  steer: false,
  interrupt: false,
  wait: false,
  close: false,
};

function boundedTimeline(
  items: AgentSessionTimelineItem[] | undefined,
): AgentSessionTimelineItem[] {
  return (items ?? [])
    .filter(
      (item) =>
        !(
          (item.kind === "activity" || item.kind === "agent_progress") &&
          isGenericActivityText(item.text)
        ),
    )
    .slice(-MAX_AGENT_TIMELINE_ITEMS);
}

function isGenericActivityText(text: string): boolean {
  const normalized = text
    .replace(/[.…]+$/g, "")
    .trim()
    .toLocaleLowerCase("es");
  return /^(analizando|pensando|razonando|procesando)( (el|la|los|las|un|una))? (siguiente )?(paso|respuesta|solicitud|petición)$/.test(
    normalized,
  );
}

function boundedActivity(items: AgentSubagentActivity[] | undefined): AgentSubagentActivity[] {
  return (items ?? []).slice(-MAX_AGENT_ACTIVITY_ITEMS);
}

function compareNodes(a: AgentSubagentThread, b: AgentSubagentThread): number {
  const pathA = (a.agent_path ?? []).join("\u0000");
  const pathB = (b.agent_path ?? []).join("\u0000");
  return pathA.localeCompare(pathB) || a.depth - b.depth || a.id.localeCompare(b.id);
}

function subagentNode(
  thread: AgentSubagentThread,
  id: string,
  parentId: string | null,
  depth: number,
): AgentTreeNode {
  return {
    id,
    providerId: thread.id,
    kind: "subagent",
    parentId,
    sourceParentId: thread.parent_id ?? null,
    depth,
    label: thread.nickname?.trim() || thread.role?.trim() || thread.id,
    role: thread.role ?? null,
    status: thread.thread_status || "unknown",
    turnStatus: thread.turn_status || "unknown",
    activity: boundedActivity(thread.activities),
    result: thread.result ?? null,
    timeline: boundedTimeline(thread.timeline),
    capabilities: { ...NO_CAPABILITIES, ...thread.capabilities },
    model: thread.model ?? null,
    reasoningEffort: thread.reasoning_effort ?? null,
    runtime: thread.runtime ?? null,
    approvalPolicy: thread.approval_policy ?? null,
    permissionMode: thread.permission_mode ?? null,
    capacity: thread.capacity ?? null,
    agentPath: thread.agent_path ?? [],
  };
}

/**
 * Returns one stable, accessible projection of the complete persisted graph.
 * Provider ids remain the keys; malformed/missing parents are shown under the
 * root without losing the original parent id in sourceParentId.
 */
export function selectAgentTree(session: AgentSession): AgentTreeNode[] {
  const root: AgentTreeNode = {
    id: session.id,
    providerId: session.id,
    kind: "root",
    parentId: null,
    sourceParentId: null,
    depth: 1,
    label: session.agent_type || "Agent",
    role: "Primary agent",
    status: session.status || "unknown",
    turnStatus: session.turn_status || "unknown",
    activity: [],
    result: null,
    timeline: boundedTimeline(session.timeline),
    capabilities: NO_CAPABILITIES,
    model: session.runtime_options?.model ?? null,
    reasoningEffort: session.runtime_options?.reasoning_effort ?? null,
    runtime: session.acp_runtime?.mode ?? null,
    approvalPolicy: null,
    permissionMode: session.permission_mode ?? null,
    capacity: null,
    agentPath: [],
  };
  const occurrences = new Map<string, number>();
  const threads = [...(session.subagents ?? [])].sort(compareNodes).map((thread) => {
    const occurrence = occurrences.get(thread.id) ?? 0;
    occurrences.set(thread.id, occurrence + 1);
    return {
      thread,
      projectionId: occurrence === 0 ? thread.id : `${thread.id}#duplicate-${occurrence}`,
    };
  });
  const firstProjectionByProviderId = new Map<string, string>([[session.id, session.id]]);
  for (const entry of threads) {
    if (!firstProjectionByProviderId.has(entry.thread.id)) {
      firstProjectionByProviderId.set(entry.thread.id, entry.projectionId);
    }
  }
  const byParent = new Map<string, typeof threads>();
  for (const entry of threads) {
    const thread = entry.thread;
    const requestedParent = thread.parent_id ?? null;
    const parent = requestedParent
      ? (firstProjectionByProviderId.get(requestedParent) ?? session.id)
      : session.id;
    if (!byParent.has(parent)) byParent.set(parent, []);
    byParent.get(parent)!.push(entry);
  }

  const result = [root];
  const visited = new Set<string>([session.id]);
  const visit = (parentId: string, depth: number) => {
    for (const entry of byParent.get(parentId) ?? []) {
      if (visited.has(entry.projectionId)) continue;
      visited.add(entry.projectionId);
      result.push(subagentNode(entry.thread, entry.projectionId, parentId, depth));
      visit(entry.projectionId, depth + 1);
    }
  };
  visit(session.id, 2);
  // A cyclic or duplicate provider payload should remain inspectable instead
  // of disappearing. Its synthetic root placement is deterministic.
  for (const entry of threads) {
    if (visited.has(entry.projectionId)) continue;
    visited.add(entry.projectionId);
    result.push(subagentNode(entry.thread, entry.projectionId, session.id, 2));
    visit(entry.projectionId, 3);
  }
  return result;
}

export function selectAgentNode(
  session: AgentSession,
  id: string | null | undefined,
): AgentTreeNode | null {
  if (!id) return null;
  return selectAgentTree(session).find((node) => node.id === id) ?? null;
}

export function selectAgentTimeline(
  session: AgentSession,
  id: string | null | undefined,
): AgentSessionTimelineItem[] {
  return selectAgentNode(session, id)?.timeline ?? [];
}

export function selectLatestAgentActivity(node: AgentTreeNode): AgentSubagentActivity | null {
  return node.activity[node.activity.length - 1] ?? null;
}

export function agentStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    pendingInit: "Pending",
    pending: "Pending",
    running: "Running",
    interrupted: "Interrupted",
    completed: "Completed",
    errored: "Errored",
    failed: "Failed",
    shutdown: "Shutdown",
    notFound: "Not found",
    exited: "Exited",
    waiting: "Waiting",
    working: "Working",
    settling: "Settling",
  };
  return labels[status] ?? (status.trim() || "Unknown");
}

export function agentCapabilityReason(
  node: AgentTreeNode,
  capability: keyof AgentSubagentCapabilities,
  readOnly: boolean,
): string | null {
  if (readOnly) return "Restored history is read-only.";
  if (!node.capabilities[capability]) return "Codex did not report this capability for the agent.";
  if (node.status === "notFound" || node.status === "shutdown") {
    return `The agent is ${agentStatusLabel(node.status).toLowerCase()}.`;
  }
  return null;
}
