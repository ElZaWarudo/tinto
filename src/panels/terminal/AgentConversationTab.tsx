import { useEffect, useState, type PointerEvent } from "react";
import type { IDockviewPanelHeaderProps } from "dockview-react";

function tabParts(title: string | undefined): { context: string; conversation: string } {
  const [agent = "Agent", project = "Proyecto", ...conversation] = (title ?? "").split(" · ");
  return {
    context: `${agent} · ${project}`,
    conversation: conversation.join(" · ") || "Nueva conversación",
  };
}

export function AgentConversationTab({ api }: IDockviewPanelHeaderProps) {
  const [title, setTitle] = useState(api.title);

  useEffect(() => {
    const disposable = api.onDidTitleChange(({ title: nextTitle }) => setTitle(nextTitle));
    return () => disposable.dispose();
  }, [api]);

  const parts = tabParts(title);
  const stopCloseDrag = (event: PointerEvent<HTMLButtonElement>) => event.stopPropagation();

  return (
    <div
      className="agent-conversation-tab"
      onAuxClick={(event) => {
        if (event.button === 1) api.close();
      }}
    >
      <span className="agent-conversation-tab__text">
        <strong>{parts.context}</strong>
        <small>{parts.conversation}</small>
      </span>
      <button
        className="agent-conversation-tab__close"
        type="button"
        aria-label={`Cerrar ${parts.conversation}`}
        onPointerDown={stopCloseDrag}
        onClick={(event) => {
          event.stopPropagation();
          api.close();
        }}
      >
        ×
      </button>
    </div>
  );
}
