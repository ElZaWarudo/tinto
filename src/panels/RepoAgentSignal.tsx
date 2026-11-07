import { repoSignal, SIGNAL_LABEL } from "../agent/sessionSignal";
import { useAgentSessionState } from "../agent/sessionStore";

/** The repo's agent sessions in one badge: te necesita, trabajando or listo. */
export function RepoAgentSignal({ repo }: { repo: string }) {
  const { sessions } = useAgentSessionState();
  const found = repoSignal(Object.values(sessions), repo);
  if (!found) return null;
  const label = SIGNAL_LABEL[found.signal];
  const text = found.count > 1 ? `${label} (${found.count})` : label;
  return (
    <span
      className={`agent-signal agent-signal--${found.signal}`}
      data-testid="agent-signal"
      title={`Agents: ${text}`}
    >
      {text}
    </span>
  );
}
