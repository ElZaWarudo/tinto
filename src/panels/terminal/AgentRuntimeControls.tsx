import { useEffect, useMemo, useRef, useState } from "react";
import type {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  ReactNode,
  RefObject,
} from "react";
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
  reasoningSupportedByModel,
  runtimeFastTier,
  speedSupportedByModel,
  type CodexModelSelection,
  type CodexReasoningSelection,
  type CodexSpeedSelection,
} from "./agentRuntimeCatalog";
import {
  createRuntimePresetId,
  loadRuntimePresets,
  runtimePresetMatches,
  saveRuntimePresets,
  type RuntimePreset,
  type RuntimePresetIcon,
} from "./runtimePresets";

export type CodexRuntimeMenu = "presets" | "reasoning" | "model" | "speed" | "summary" | null;

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
  onPresetApply,
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
  onPresetApply: (preset: RuntimePreset) => void;
  onReasoningChange: (value: CodexReasoningSelection) => void;
  onRefreshCatalog: () => void;
  onSpeedChange: (value: CodexSpeedSelection) => void;
  providerLabel: string;
  reasoning: CodexReasoningSelection;
  speed: CodexSpeedSelection;
}) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const summaryTriggerRef = useRef<HTMLButtonElement | null>(null);
  const previousMenuRef = useRef<CodexRuntimeMenu>(null);
  const lastTriggerRef = useRef<RefObject<HTMLButtonElement | null> | null>(null);
  const [modelQuery, setModelQuery] = useState("");
  const [presets, setPresets] = useState(loadRuntimePresets);
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(null);
  const [presetDraft, setPresetDraft] = useState<RuntimePreset | null>(null);
  const [presetError, setPresetError] = useState<string | null>(null);
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
  const selectedPreset = presets.find((preset) => preset.id === selectedPresetId);
  const activePreset =
    selectedPreset && runtimePresetMatches(selectedPreset, model, reasoning, speed)
      ? selectedPreset
      : presets.find((preset) => runtimePresetMatches(preset, model, reasoning, speed));
  useEffect(() => {
    const previous = previousMenuRef.current;
    previousMenuRef.current = menu;
    if (!menu) {
      const previousTrigger = lastTriggerRef.current ?? (previous ? summaryTriggerRef : null);
      queueMicrotask(() => {
        previousTrigger?.current?.focus();
        lastTriggerRef.current = null;
        setModelQuery("");
        setPresetDraft(null);
        setPresetError(null);
      });
      return;
    }
    const frame = requestAnimationFrame(() => {
      const checked = panelRef.current?.querySelector<HTMLInputElement>(
        'input[type="radio"]:checked:not(:disabled)',
      );
      const first = panelRef.current?.querySelector<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), select:not(:disabled)",
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

  const toggle = (
    next: Exclude<CodexRuntimeMenu, null>,
    trigger?: RefObject<HTMLButtonElement | null>,
  ) => {
    lastTriggerRef.current = trigger ?? null;
    onMenuChange(menu === next ? null : next);
  };
  const closeAfterSelection = () => {
    if (menu !== "summary") onMenuChange(null);
  };
  const storePresets = (next: RuntimePreset[]) => {
    const saved = saveRuntimePresets(next);
    setPresets(saved);
  };
  const startNewPreset = () => {
    setPresetError(null);
    setPresetDraft({
      id: "",
      name: "",
      model,
      reasoning,
      speed,
      icon: nextPresetIcon(presets),
      color: nextPresetColor(presets),
      favorite: false,
    });
  };
  const savePresetDraft = () => {
    if (!presetDraft) return;
    const name = presetDraft.name.trim();
    if (!name) {
      setPresetError("Escribe un nombre para la configuración guardada.");
      return;
    }
    if (
      presets.some(
        (preset) =>
          preset.id !== presetDraft.id &&
          preset.name.toLocaleLowerCase() === name.toLocaleLowerCase(),
      )
    ) {
      setPresetError("Ya existe una configuración guardada con ese nombre.");
      return;
    }
    const saved = {
      ...presetDraft,
      id: presetDraft.id || createRuntimePresetId(name),
      name,
    };
    storePresets(
      presetDraft.id
        ? presets.map((preset) => (preset.id === presetDraft.id ? saved : preset))
        : [...presets, saved],
    );
    setPresetDraft(null);
    setPresetError(null);
    setSelectedPresetId(saved.id);
    onPresetApply(saved);
  };
  const deletePresetDraft = () => {
    if (!presetDraft?.id) return;
    storePresets(presets.filter((preset) => preset.id !== presetDraft.id));
    if (selectedPresetId === presetDraft.id) setSelectedPresetId(null);
    setPresetDraft(null);
    setPresetError(null);
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
  const showPresets = menu === "presets";
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
      <button
        aria-controls={panelId}
        aria-expanded={menu === "presets"}
        aria-haspopup="dialog"
        className="agent-panel__runtime-summary"
        disabled={disabled}
        onClick={() => toggle("presets", summaryTriggerRef)}
        ref={summaryTriggerRef}
        type="button"
      >
        <span>Configuración guardada</span>
        <strong>
          {activePreset?.name ?? "Personalizado"} · {modelDisplay} · {reasoningDisplay} ·{" "}
          {codexSpeedLabel(speed)}
        </strong>
        <span aria-hidden="true" className="agent-panel__runtime-chevron">
          ▾
        </span>
      </button>
      <button
        aria-pressed={speed === "fast"}
        className="agent-panel__runtime-fast"
        disabled={disabled || !speedSupportedByModel(catalog, model, "fast")}
        onClick={() => {
          setSelectedPresetId(null);
          onSpeedChange(speed === "fast" ? "standard" : "fast");
        }}
        title={speed === "fast" ? "Desactivar modo rápido" : "Activar modo rápido"}
        type="button"
      >
        Rápido
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
          data-runtime-menu={menu}
          id={panelId}
          onKeyDown={onInspectorKeyDown}
          ref={panelRef}
          role="dialog"
        >
          <header className="agent-panel__runtime-inspector-head">
            <strong>{runtimeInspectorTitle(menu)}</strong>
            <div>
              {showPresets && !presetDraft && (
                <button className="agent-panel__runtime-new" onClick={startNewPreset} type="button">
                  + Nuevo
                </button>
              )}
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
            {showPresets && (
              <RuntimePresetPanel
                activePresetId={activePreset?.id ?? null}
                catalog={catalog}
                draft={presetDraft}
                error={presetError}
                onApply={(preset) => {
                  setSelectedPresetId(preset.id);
                  onPresetApply(preset);
                  onMenuChange(null);
                }}
                onCancel={() => {
                  setPresetDraft(null);
                  setPresetError(null);
                }}
                onConfigure={() => onMenuChange("summary")}
                onDelete={deletePresetDraft}
                onDraftChange={(draft) => {
                  setPresetDraft(draft);
                  setPresetError(null);
                }}
                onEdit={(preset) => {
                  setPresetDraft({ ...preset });
                  setPresetError(null);
                }}
                onSave={savePresetDraft}
                onSetFavorite={(preset) =>
                  storePresets(
                    presets.map((item) => ({
                      ...item,
                      favorite: item.id === preset.id ? !preset.favorite : false,
                    })),
                  )
                }
                presets={presets}
              />
            )}
            {showModels && (
              <RuntimeChoiceGroup
                choices={filteredModelChoices}
                empty={normalizedQuery ? "Sin coincidencias" : undefined}
                legend="Modelo"
                name={`${idBase}-model`}
                onChange={(value) => {
                  setSelectedPresetId(null);
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
                  setSelectedPresetId(null);
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
                  setSelectedPresetId(null);
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

function RuntimePresetPanel({
  activePresetId,
  catalog,
  draft,
  error,
  onApply,
  onCancel,
  onConfigure,
  onDelete,
  onDraftChange,
  onEdit,
  onSave,
  onSetFavorite,
  presets,
}: {
  activePresetId: string | null;
  catalog: AgentRuntimeCatalog | null;
  draft: RuntimePreset | null;
  error: string | null;
  onApply: (preset: RuntimePreset) => void;
  onCancel: () => void;
  onConfigure: () => void;
  onDelete: () => void;
  onDraftChange: (draft: RuntimePreset) => void;
  onEdit: (preset: RuntimePreset) => void;
  onSave: () => void;
  onSetFavorite: (preset: RuntimePreset) => void;
  presets: RuntimePreset[];
}) {
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const confirmDelete = Boolean(draft?.id) && confirmDeleteId === draft?.id;

  if (draft) {
    const modelChoices = runtimeModelChoices(catalog, draft.model);
    const reasoningChoices = runtimeReasoningChoices(catalog, draft.model, draft.reasoning);
    return (
      <div className="agent-panel__preset-editor">
        <label>
          <span>Nombre</span>
          <input
            autoComplete="off"
            autoFocus
            maxLength={40}
            onChange={(event) => onDraftChange({ ...draft, name: event.target.value })}
            value={draft.name}
          />
        </label>
        <div className="agent-panel__preset-appearance">
          <fieldset>
            <legend>Icono</legend>
            <div className="agent-panel__preset-icon-options">
              {PRESET_ICONS.map((choice) => (
                <button
                  aria-label={choice.label}
                  aria-pressed={draft.icon === choice.value}
                  key={choice.value}
                  onClick={() => onDraftChange({ ...draft, icon: choice.value })}
                  title={choice.label}
                  type="button"
                >
                  <PresetIcon icon={choice.value} />
                </button>
              ))}
            </div>
          </fieldset>
          <fieldset>
            <legend>Color</legend>
            <div className="agent-panel__preset-color-options">
              {PRESET_COLORS.map((choice) => (
                <button
                  aria-label={`Color ${choice.label}`}
                  aria-pressed={draft.color === choice.value}
                  key={choice.value}
                  onClick={() => onDraftChange({ ...draft, color: choice.value })}
                  style={presetVisualStyle(choice.value)}
                  title={choice.label}
                  type="button"
                />
              ))}
              <label className="agent-panel__preset-custom-color">
                <span>Personalizado</span>
                <input
                  aria-label="Color personalizado"
                  onChange={(event) => onDraftChange({ ...draft, color: event.target.value })}
                  type="color"
                  value={draft.color}
                />
              </label>
            </div>
          </fieldset>
        </div>
        <div className="agent-panel__preset-fields">
          <label>
            <span>Modelo</span>
            <select
              onChange={(event) =>
                onDraftChange({
                  ...draft,
                  model: event.target.value,
                  reasoning: "auto",
                  speed: "standard",
                })
              }
              value={draft.model}
            >
              {modelChoices.map((choice) => (
                <option disabled={choice.disabled} key={choice.value} value={choice.value}>
                  {choice.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Razonamiento</span>
            <select
              onChange={(event) => onDraftChange({ ...draft, reasoning: event.target.value })}
              value={draft.reasoning}
            >
              {reasoningChoices.map((choice) => (
                <option disabled={choice.disabled} key={choice.value} value={choice.value}>
                  {choice.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Perfil</span>
            <select
              onChange={(event) =>
                onDraftChange({
                  ...draft,
                  speed: event.target.value === "fast" ? "fast" : "standard",
                })
              }
              value={draft.speed}
            >
              {runtimeSpeedChoices(catalog, draft.model, draft.speed).map((choice) => (
                <option disabled={choice.disabled} key={choice.value} value={choice.value}>
                  {choice.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        {error && (
          <span className="agent-panel__preset-error" role="alert">
            {error}
          </span>
        )}
        <div className="agent-panel__preset-editor-actions">
          {draft.id && (
            <button
              className={confirmDelete ? "agent-panel__preset-delete--confirm" : undefined}
              onClick={() => {
                if (confirmDelete) onDelete();
                else setConfirmDeleteId(draft.id);
              }}
              type="button"
            >
              {confirmDelete ? "Confirmar borrado" : "Eliminar"}
            </button>
          )}
          <span />
          <button onClick={onCancel} type="button">
            Cancelar
          </button>
          <button className="agent-panel__preset-save" onClick={onSave} type="button">
            Guardar y aplicar
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="agent-panel__preset-list">
      <div role="list">
        {presets.map((preset) => {
          const unavailable = runtimePresetUnavailableReason(preset, catalog);
          return (
            <div
              className={`agent-panel__preset-row${
                activePresetId === preset.id ? " agent-panel__preset-row--active" : ""
              }`}
              key={preset.id}
              role="listitem"
              style={presetVisualStyle(preset.color)}
            >
              <button
                className="agent-panel__preset-apply"
                disabled={Boolean(unavailable)}
                onClick={() => onApply(preset)}
                title={unavailable ?? `Aplicar ${preset.name}`}
                type="button"
              >
                <span
                  aria-hidden="true"
                  className="agent-panel__preset-mark"
                  style={presetVisualStyle(preset.color)}
                >
                  <PresetIcon icon={preset.icon} />
                </span>
                <span className="agent-panel__preset-copy">
                  <strong>{preset.name}</strong>
                  <small>{unavailable ?? runtimePresetSummary(preset, catalog)}</small>
                </span>
                {activePresetId === preset.id && (
                  <span
                    aria-label="Configuración guardada activa"
                    className="agent-panel__preset-check"
                  >
                    ✓
                  </span>
                )}
              </button>
              <button
                aria-label={
                  preset.favorite
                    ? `Quitar ${preset.name} como configuración guardada favorita`
                    : `Usar ${preset.name} al iniciar los chats`
                }
                aria-pressed={preset.favorite}
                className="agent-panel__preset-favorite"
                onClick={() => onSetFavorite(preset)}
                title={
                  preset.favorite ? "Configuración guardada favorita" : "Usar al iniciar los chats"
                }
                type="button"
              >
                {preset.favorite ? "★" : "☆"}
              </button>
              <button
                aria-label={`Editar configuración guardada ${preset.name}`}
                className="agent-panel__preset-edit"
                onClick={() => onEdit(preset)}
                title={`Editar ${preset.name}`}
                type="button"
              >
                Editar
              </button>
            </div>
          );
        })}
      </div>
      <button className="agent-panel__preset-configure" onClick={onConfigure} type="button">
        Configurar ejecución…
      </button>
    </div>
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

function runtimePresetSummary(preset: RuntimePreset, catalog: AgentRuntimeCatalog | null): string {
  return [
    codexModelLabel(catalog, preset.model),
    codexReasoningLabel(preset.reasoning),
    codexSpeedLabel(preset.speed),
  ].join(" · ");
}

function runtimePresetUnavailableReason(
  preset: RuntimePreset,
  catalog: AgentRuntimeCatalog | null,
): string | null {
  if (!reasoningSupportedByModel(catalog, preset.model, preset.reasoning)) {
    return "Razonamiento no compatible con el modelo";
  }
  if (!speedSupportedByModel(catalog, preset.model, preset.speed)) {
    return "El perfil rápido no está disponible para este modelo";
  }
  return null;
}

const PRESET_ICONS: Array<{ value: RuntimePresetIcon; label: string }> = [
  { value: "calendar", label: "Ritual" },
  { value: "target", label: "Enfoque" },
  { value: "bolt", label: "Impulso" },
  { value: "code", label: "Código" },
  { value: "compass", label: "Rumbo" },
  { value: "spark", label: "Tueste" },
];

const PRESET_COLORS = [
  { value: "#2dd4bf", label: "menta" },
  { value: "#8b5cf6", label: "violeta" },
  { value: "#f59e0b", label: "ámbar" },
  { value: "#3b82f6", label: "azul" },
  { value: "#f43f5e", label: "rosa" },
  { value: "#84cc16", label: "lima" },
] as const;

function nextPresetIcon(presets: RuntimePreset[]): RuntimePresetIcon {
  return PRESET_ICONS[presets.length % PRESET_ICONS.length].value;
}

function nextPresetColor(presets: RuntimePreset[]): string {
  return PRESET_COLORS[presets.length % PRESET_COLORS.length].value;
}

function presetVisualStyle(color: string): CSSProperties {
  return { "--preset-color": color } as CSSProperties;
}

function PresetIcon({ icon }: { icon: RuntimePresetIcon }) {
  const paths: Record<RuntimePresetIcon, ReactNode> = {
    calendar: (
      <>
        <path d="M5 10h12v3.5A5.5 5.5 0 0 1 11.5 19h-1A5.5 5.5 0 0 1 5 13.5zM17 11h1.2a2.3 2.3 0 0 1 0 4.6H17" />
        <path d="M8 7c-1-1.2.8-1.9 0-3M12 7c-1-1.2.8-1.9 0-3" />
      </>
    ),
    target: (
      <>
        <path d="M18.7 7.2A8.5 8.5 0 1 1 15.8 4.4M19 3v5h-5" />
        <path d="M14.8 8.7c1.8 2.1.7 5.7-2.1 6.6-2.5.8-4.4-.7-4.2-2.8.2-2.8 3.8-5.8 6.3-3.8zM13.8 9.3c-1.4 1.3-2.1 3.1-2 5" />
      </>
    ),
    bolt: (
      <>
        <path d="M15.5 3.2c3.3 2.4 3 7.7-.6 12.1-3.2 4-7.7 5.3-10.1 2.7-2.5-2.7-.8-7.2 2.6-10.8 2.8-3 5.9-5.2 8.1-4z" />
        <path d="m13.5 5-4 6h3l-2 6 5-7h-3z" />
      </>
    ),
    code: (
      <>
        <path d="m8 7-4 5 4 5M16 7l4 5-4 5" />
        <path d="M13.8 8.3c1.7 1.7.9 5.5-1.5 7.1-2 1.4-3.8.1-3.4-2 .4-2.3 3-6.5 4.9-5.1zM12.9 9.2c-1.1 1.5-1.4 3.2-1.2 5" />
      </>
    ),
    compass: (
      <>
        <path d="M12 2.8 21.2 12 12 21.2 2.8 12z" />
        <path d="M15.2 8.8 13.5 14l-4.7 1.2 1.7-5.2zM13.8 9.5c-1.2 1.1-1.8 2.7-1.7 4.3" />
      </>
    ),
    spark: (
      <>
        <path d="M12 2.5c.7 4.2 2.8 6.4 7 7.1-4.2.7-6.3 2.9-7 7.1-.7-4.2-2.8-6.4-7-7.1 4.2-.7 6.3-2.9 7-7.1z" />
        <path d="M12.9 8.3c1.4 1 .7 3.4-.8 4.2-1.3.7-2.4-.1-2.1-1.5.3-1.5 1.8-3.5 2.9-2.7zM12.4 8.9c-.7.9-.9 1.8-.8 2.8" />
        <path d="M18.5 15.5v3M17 17h3" />
      </>
    ),
  };
  return (
    <svg aria-hidden="true" className="agent-panel__preset-icon-glyph" viewBox="0 0 24 24">
      {paths[icon]}
    </svg>
  );
}

function runtimeInspectorTitle(menu: Exclude<CodexRuntimeMenu, null>): string {
  if (menu === "presets") return "Configuraciones guardadas de ejecución";
  if (menu === "summary") return "Ejecución del próximo turno";
  if (menu === "model") return "Modelo del próximo turno";
  if (menu === "reasoning") return "Razonamiento del próximo turno";
  return "Perfil del próximo turno";
}
