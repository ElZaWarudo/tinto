import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DeliveryGuideRule, DeliveryGuideView } from "../../delivery/types";
import { GuidesSection } from "./DeliveryGuides";

const client = vi.hoisted(() => ({
  getDeliveryGuide: vi.fn(),
  getDeliveryGuideCandidates: vi.fn(),
  getDeliveryRepoSettings: vi.fn(),
  setDeliveryRepoSettings: vi.fn(),
  collectDeliveryGuideComments: vi.fn(),
  requestDeliveryGuide: vi.fn(),
  decideDeliveryGuide: vi.fn(),
  onDeliveryChanged: vi.fn(),
}));
vi.mock("../../delivery/client", () => client);

const rule = (id: string, text: string, over: Partial<DeliveryGuideRule> = {}) => ({
  id,
  kind: "visual" as const,
  rule: text,
  applies_to: [],
  stages: [],
  why: "",
  example: "",
  sources: [],
  seen: 1,
  ...over,
});

const view = (over: Partial<DeliveryGuideView> = {}): DeliveryGuideView => ({
  people: [{ login: "ana", kinds: ["visual"] }],
  rules: [],
  proposal: null,
  comments: 0,
  pending: 0,
  collected_at_ms: null,
  ...over,
});

const repo = { path: "C:\\work\\agentos", distro: null, label: "agentos" };

beforeEach(() => {
  vi.clearAllMocks();
  client.getDeliveryGuideCandidates.mockResolvedValue(["ana", "luis"]);
  client.getDeliveryRepoSettings.mockResolvedValue({ checks: ["npm test"] });
  client.setDeliveryRepoSettings.mockResolvedValue({});
  client.onDeliveryChanged.mockImplementation((callback: () => void) => {
    changed = callback;
    return Promise.resolve(() => {});
  });
});

/** Delivery's change event, as a coordinator's proposal fires it. */
let changed = () => {};

describe("GuidesSection", () => {
  it("makes reviewers guides and keeps the rest of the repo's settings", async () => {
    const user = userEvent.setup();
    client.getDeliveryGuide.mockResolvedValue(view());
    render(<GuidesSection repo={repo} />);
    expect(await screen.findByRole("combobox", { name: "Papel de ana" })).toHaveValue("visual");
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Papel de luis" }),
      "both",
    );
    await user.click(screen.getByRole("button", { name: "Guardar guías" }));
    expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(repo.path, {
      checks: ["npm test"],
      guides: [
        { login: "ana", kinds: ["visual"] },
        { login: "luis", kinds: ["visual", "technical"] },
      ],
    });
  });

  it("shows a proposal as it arrives: what it adds, changes and retires, to approve", async () => {
    const user = userEvent.setup();
    const before = view({ rules: [rule("V-1", "Espaciado de 4px"), rule("V-2", "Sin sombras")] });
    client.getDeliveryGuide.mockResolvedValueOnce(before).mockResolvedValue(
      view({
        rules: [rule("V-1", "Espaciado de 4px"), rule("V-2", "Sin sombras")],
        proposal: {
          rules: [
            rule("V-1", "Espaciado de 4px", { seen: 3 }),
            rule("V-3", "Los estados vacíos dicen qué hacer"),
          ],
          through: 9,
          note: "Dos comentarios nuevos",
          requested_by: "coord",
          requested_at_ms: 1,
        },
      }),
    );
    client.decideDeliveryGuide.mockResolvedValue(
      view({ rules: [rule("V-1", "Espaciado de 4px", { seen: 3 })] }),
    );
    render(<GuidesSection repo={repo} />);
    expect(await screen.findByText("Sin sombras")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Reglas propuestas" })).toBeNull();
    // The coordinator proposes while the page is open.
    await act(async () => changed());
    const proposal = await screen.findByRole("group", { name: "Reglas propuestas" });
    const marks = within(proposal)
      .getAllByRole("listitem")
      .map((item) => item.querySelector(".delivery-guide-rule__mark")?.textContent);
    expect(marks).toEqual(["Cambia", "Nueva", "Se retira"]);
    await user.click(within(proposal).getByRole("button", { name: "Aprobar" }));
    expect(client.decideDeliveryGuide).toHaveBeenCalledWith(repo.path, true);
    expect(await screen.findByText("Reglas aprobadas.")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Reglas propuestas" })).toBeNull();
  });
});
