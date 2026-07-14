import { useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode, RefObject } from "react";
import type {
  AgentRuntimeCatalog,
  AgentRuntimeModel,
  AgentRuntimeReasoningEffort,
} from "../../bus/contract";
import {
  codexModelLabel,
  codexReasoningLabel,
  codexSpeedLabel,
  effectiveRuntimeModel,
  runtimeFastTier,
  type CodexModelSelection,
  type CodexReasoningSelection,
  type CodexSpeedSelection,
} from "./agentRuntimeCatalog";

export type CodexRuntimeMenu = "reasoning" | "model" | "speed" | "summary" | null;

interface RuntimeChoice {
  value: string;
  label: string;
  meta: string;
  mark?: string;
  disabled?: boolean;
}

export function AgentRuntimeControls({
  catalog,
  disabled,
  idBase,
  menu,
  model,
  notice,
  onMenuChange,
  onModelChange,
  onReasoningChange,
  onRefreshCatalog,
  onSpeedChange,
  providerLabel,
  reasoning,
  speed,
}: {
  catalog: AgentRuntimeCatalog | null;
  disabled: boolean;
  idBase: string;
  menu: CodexRuntimeMenu;
  model: CodexModelSelection;
  notice: string | null;
  onMenuChange: (menu: CodexRuntimeMenu) => void;
  onModelChange: (value: CodexModelSelection) => void;
  onReasoningChange: (value: CodexReasoningSelection) => void;
  onRefreshCatalog: () => void;
  onSpeedChange: (value: CodexSpeedSelection) => void;
  providerLabel: string;
  reasoning: CodexReasoningSelection;
  speed: CodexSpeedSelection;
}) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const modelTriggerRef = useRef<HTMLButtonElement | null>(null);
  const reasoningTriggerRef = useRef<HTMLButtonElement | null>(null);
  const speedTriggerRef = useRef<HTMLButtonElement | null>(null);
  const summaryTriggerRef = useRef<HTMLButtonElement | null>(null);
  const previousMenuRef = useRef<CodexRuntimeMenu>(null);
  const [modelQuery, setModelQuery] = useState("");
  const panelId = `${idBase}-runtime-inspector`;
  const models = catalog?.models ?? [];
  const modelChoices = useMemo(() => runtimeModelChoices(catalog, model), [catalog, model]);
  const reasoningChoices = useMemo(
    () => runtimeReasoningChoices(catalog, model, reasoning),
    [catalog, model, reasoning],
  );
  const speedChoices = useMemo(
    () => runtimeSpeedChoices(catalog, model, speed),
    [catalog, model, speed],
  );
  const normalizedQuery = modelQuery.trim().toLocaleLowerCase();
  const filteredModelChoices = normalizedQuery
    ? modelChoices.filter((choice) =>
        `${choice.label} ${choice.meta}`.toLocaleLowerCase().includes(normalizedQuery),
      )
    : modelChoices;
  useEffect(() => {
    const previous = previousMenuRef.current;
    previousMenuRef.current = menu;
    if (!menu) {
      const previousTrigger =
        previous === "model"
          ? modelTriggerRef
          : previous === "reasoning"
            ? reasoningTriggerRef
            : previous === "speed"
              ? speedTriggerRef
              : previous === "summary"
                ? summaryTriggerRef
                : null;
      queueMicrotask(() => {
        previousTrigger?.current?.focus();
        setModelQuery("");
      });
      return;
    }
    const frame = requestAnimationFrame(() => {
      const checked = panelRef.current?.querySelector<HTMLInputElement>(
        'input[type="radio"]:checked:not(:disabled)',
      );
      const first = panelRef.current?.querySelector<HTMLInputElement>(
        'input[type="radio"]:not(:disabled)',
      );
      (checked ?? first)?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onMenuChange(null);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [menu, onMenuChange]);

  const toggle = (next: Exclude<CodexRuntimeMenu, null>) => {
    onMenuChange(menu === next ? null : next);
  };
  const closeAfterSelection = () => {
    if (menu !== "summary") onMenuChange(null);
  };
  const onInspectorKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    onMenuChange(null);
  };
  const stateMark =
    catalog?.status === "loading"
      ? "ACTUALIZANDO"
      : catalog?.status === "error"
        ? "CATÁLOGO NO DISPONIBLE"
        : null;
  const showModels = menu === "model" || menu === "summary";
  const showReasoning = menu === "reasoning" || menu === "summary";
  const showSpeed = menu === "speed" || menu === "summary";
  const resolvedModel = effectiveRuntimeModel(catalog, model);
  const modelDisplay =
    model === "auto" && resolvedModel
      ? `Predeterminado · ${resolvedModel.display_name}`
      : codexModelLabel(catalog, model);
  const reasoningDisplay =
    reasoning === "auto" && resolvedModel
      ? `Predeterminado · ${codexReasoningLabel(resolvedModel.default_reasoning_effort)}`
      : codexReasoningLabel(reasoning);

  return (
    <div
      className="agent-panel__runtime"
      aria-label={`Ejecución de ${providerLabel} para el próximo turno`}
      data-runtime-provider={providerLabel.toLocaleLowerCase()}
      ref={rootRef}
    >
      <div className="agent-panel__runtime-band">
        <RuntimeTrigger
          controls={panelId}
          disabled={disabled}
          expanded={menu === "model"}
          label="Modelo"
          onClick={() => toggle("model")}
          ref={modelTriggerRef}
          value={modelDisplay}
        />
        <RuntimeTrigger
          controls={panelId}
          disabled={disabled}
          expanded={menu === "reasoning"}
          label="Razonamiento"
          onClick={() => toggle("reasoning")}
          ref={reasoningTriggerRef}
          value={reasoningDisplay}
        />
        <RuntimeTrigger
          controls={panelId}
          disabled={disabled}
          expanded={menu === "speed"}
          label="Perfil"
          onClick={() => toggle("speed")}
          ref={speedTriggerRef}
          value={codexSpeedLabel(speed)}
        />
      </div>
      <button
        aria-controls={panelId}
        aria-expanded={menu === "summary"}
        aria-haspopup="dialog"
        className="agent-panel__runtime-summary"
        disabled={disabled}
        onClick={() => toggle("summary")}
        ref={summaryTriggerRef}
        type="button"
      >
        <span>Ejecución</span>
        <strong>
          {modelDisplay} / {reasoningDisplay} / {codexSpeedLabel(speed)}
        </strong>
        <span aria-hidden="true" className="agent-panel__runtime-chevron">
          ▾
        </span>
      </button>
      {notice && (
        <span className="agent-panel__runtime-notice" role="status" aria-live="polite">
          {notice}
        </span>
      )}
      {menu && (
        <div
          aria-label="Configuración de ejecución"
          className="agent-panel__runtime-inspector"
          id={panelId}
          onKeyDown={onInspectorKeyDown}
          ref={panelRef}
          role="dialog"
        >
          <header className="agent-panel__runtime-inspector-head">
            <strong>{runtimeInspectorTitle(menu)}</strong>
            <div>
              {stateMark && <span className="agent-panel__runtime-state">{stateMark}</span>}
              {catalog?.status === "error" && (
                <button onClick={onRefreshCatalog} type="button">
                  Reintentar
                </button>
              )}
              <button
                aria-label="Cerrar configuración de ejecución"
                onClick={() => onMenuChange(null)}
                type="button"
              >
                ×
              </button>
            </div>
          </header>
          <div className="agent-panel__runtime-inspector-body">
            {showModels && (
              <RuntimeChoiceGroup
                choices={filteredModelChoices}
                empty={normalizedQuery ? "Sin coincidencias" : undefined}
                legend="Modelo"
                name={`${idBase}-model`}
                onChange={(value) => {
                  onModelChange(value);
                  closeAfterSelection();
                }}
                search={
                  models.length > 8 ? (
                    <input
                      aria-label="Filtrar modelos"
                      onChange={(event) => setModelQuery(event.target.value)}
                      placeholder="Filtrar modelos"
                      type="search"
                      value={modelQuery}
                    />
                  ) : undefined
                }
                value={model}
              />
            )}
            {showReasoning && (
              <RuntimeChoiceGroup
                choices={reasoningChoices}
                legend="Razonamiento"
                name={`${idBase}-reasoning`}
                onChange={(value) => {
                  onReasoningChange(value);
                  closeAfterSelection();
                }}
                value={reasoning}
              />
            )}
            {showSpeed && (
              <RuntimeChoiceGroup
                choices={speedChoices}
                legend="Perfil"
                name={`${idBase}-speed`}
                onChange={(value) => {
                  onSpeedChange(value === "fast" ? "fast" : "standard");
                  closeAfterSelection();
                }}
                value={speed}
              />
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function RuntimeTrigger({
  controls,
  disabled,
  expanded,
  label,
  onClick,
  ref,
  value,
}: {
  controls: string;
  disabled: boolean;
  expanded: boolean;
  label: string;
  onClick: () => void;
  ref: RefObject<HTMLButtonElement | null>;
  value: string;
}) {
  return (
    <button
      aria-controls={controls}
      aria-expanded={expanded}
      aria-haspopup="dialog"
      aria-label={`${label}: ${value}`}
      className="agent-panel__runtime-cell"
      disabled={disabled}
      onClick={onClick}
      ref={ref}
      type="button"
    >
      <span>{label}</span>
      <strong>{value}</strong>
      <span aria-hidden="true" className="agent-panel__runtime-chevron">
        ▾
      </span>
    </button>
  );
}

function RuntimeChoiceGroup({
  choices,
  empty,
  legend,
  name,
  onChange,
  search,
  value,
}: {
  choices: RuntimeChoice[];
  empty?: string;
  legend: string;
  name: string;
  onChange: (value: string) => void;
  search?: ReactNode;
  value: string;
}) {
  return (
    <fieldset className="agent-panel__runtime-group">
      <legend>{legend}</legend>
      {search}
      <div className="agent-panel__runtime-options">
        {choices.length === 0 && empty ? (
          <span className="agent-panel__runtime-empty">{empty}</span>
        ) : (
          choices.map((choice) => (
            <label
              className={`agent-panel__runtime-option${
                value === choice.value ? " agent-panel__runtime-option--active" : ""
              }${choice.disabled ? " agent-panel__runtime-option--disabled" : ""}`}
              key={choice.value}
            >
              <input
                checked={value === choice.value}
                disabled={choice.disabled}
                name={name}
                onChange={() => onChange(choice.value)}
                type="radio"
                value={choice.value}
              />
              <span className="agent-panel__runtime-option-copy">
                <strong>{choice.label}</strong>
                <small>{choice.meta}</small>
              </span>
              {choice.mark && (
                <span className="agent-panel__runtime-option-mark">{choice.mark}</span>
              )}
            </label>
          ))
        )}
      </div>
    </fieldset>
  );
}

function runtimeModelChoices(
  catalog: AgentRuntimeCatalog | null,
  selected: CodexModelSelection,
): RuntimeChoice[] {
  const models = catalog?.models ?? [];
  const defaultModel = effectiveRuntimeModel(catalog, "auto");
  const choices: RuntimeChoice[] = [
    {
      value: "auto",
      label: "Predeterminado",
      meta: defaultModel?.display_name ?? catalogStateLabel(catalog),
      mark: "AUTO",
    },
    ...models.map((model) => ({
      value: model.id,
      label: model.display_name || model.id,
      meta: modelChoiceMeta(model),
      mark: model.is_default ? "PREDETERMINADO" : undefined,
    })),
  ];
  if (selected !== "auto" && !models.some((model) => model.id === selected)) {
    choices.push({
      value: selected,
      label: selected,
      meta: "Modelo activo fuera del catálogo",
      mark: "NO CATALOGADO",
    });
  }
  return choices;
}

function runtimeReasoningChoices(
  catalog: AgentRuntimeCatalog | null,
  selectedModel: CodexModelSelection,
  selectedReasoning: CodexReasoningSelection,
): RuntimeChoice[] {
  const model = effectiveRuntimeModel(catalog, selectedModel);
  const efforts = model?.supported_reasoning_efforts ?? [];
  const choices: RuntimeChoice[] = [
    {
      value: "auto",
      label: "Predeterminado",
      meta: model
        ? codexReasoningLabel(model.default_reasoning_effort)
        : catalogStateLabel(catalog),
      mark: "AUTO",
    },
    ...efforts.map((effort) => reasoningChoice(effort)),
  ];
  if (
    selectedReasoning !== "auto" &&
    !efforts.some((effort) => effort.value === selectedReasoning)
  ) {
    choices.push({
      value: selectedReasoning,
      label: codexReasoningLabel(selectedReasoning),
      meta: selectedReasoning,
      mark: catalog?.status === "ready" ? "NO COMPATIBLE" : "NO CATALOGADO",
    });
  }
  return choices;
}

function runtimeSpeedChoices(
  catalog: AgentRuntimeCatalog | null,
  selectedModel: CodexModelSelection,
  selectedSpeed: CodexSpeedSelection,
): RuntimeChoice[] {
  const fastTier = runtimeFastTier(catalog, selectedModel);
  const catalogResolved = catalog?.status === "ready";
  const modelResolved = Boolean(effectiveRuntimeModel(catalog, selectedModel));
  return [
    { value: "standard", label: "Normal", meta: "STANDARD", mark: "PREDETERMINADO" },
    ...(fastTier || selectedSpeed === "fast"
      ? [
          {
            value: "fast",
            label: fastTier?.name || "Rápido",
            meta: "FAST",
            mark: !fastTier && catalogResolved && modelResolved ? "NO DISPONIBLE" : undefined,
            disabled: !fastTier && catalogResolved && modelResolved,
          },
        ]
      : []),
  ];
}

function reasoningChoice(effort: AgentRuntimeReasoningEffort): RuntimeChoice {
  return {
    value: effort.value,
    label: codexReasoningLabel(effort.value),
    meta: effort.value.toLocaleUpperCase(),
  };
}

function modelChoiceMeta(model: AgentRuntimeModel): string {
  const efforts = model.supported_reasoning_efforts.map((effort) => effort.value).join(" · ");
  return efforts || model.model || model.id;
}

function catalogStateLabel(catalog: AgentRuntimeCatalog | null): string {
  if (!catalog || catalog.status === "loading") return "Actualizando catálogo";
  if (catalog.status === "error") return "Catálogo no disponible";
  return "Codex";
}

function runtimeInspectorTitle(menu: Exclude<CodexRuntimeMenu, null>): string {
  if (menu === "summary") return "Ejecución del próximo turno";
  if (menu === "model") return "Modelo del próximo turno";
  if (menu === "reasoning") return "Razonamiento del próximo turno";
  return "Perfil del próximo turno";
}
