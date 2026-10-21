import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DeliveryIssue } from "../../delivery/types";

const client = vi.hoisted(() => ({
  getDeliveryIssue: vi.fn(),
  openDeliveryUrl: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../delivery/client", () => client);

import { JiraIssue, sanitizeIssueHtml } from "./DeliveryIssue";

const URL = "https://jira.test/browse/AGOS-455";

function issue(overrides: Partial<DeliveryIssue> = {}): DeliveryIssue {
  return {
    key: "AGOS-455",
    url: URL,
    summary: "Exportar facturas",
    status: "En ejecución",
    type: "Historia",
    priority: "Alta",
    assignee: "Ana",
    reporter: null,
    labels: ["facturas", "csv"],
    parent: { key: "AGOS-400", summary: "Facturación" },
    links: [{ type: "is blocked by", key: "AGOS-12", summary: "API", status: "Cerrada" }],
    description_html: '<p>Exporta <b>CSV</b> como en <a href="/browse/AGOS-12">AGOS-12</a></p>',
    comments: [
      { author: "Luis", created: "2026-10-20T10:00:00.000+02:00", body_html: "<p>Hecho</p>" },
    ],
    ...overrides,
  };
}

describe("JiraIssue", () => {
  beforeEach(() => {
    client.getDeliveryIssue.mockReset();
    client.openDeliveryUrl.mockClear();
  });

  it("reads the issue and shows its fields, description and comments", async () => {
    let answer: (read: DeliveryIssue) => void = () => {};
    client.getDeliveryIssue.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    render(<JiraIssue taskId="t1" issueUrl={URL} />);
    expect(screen.getByText("Leyendo la incidencia en Jira…")).toBeInTheDocument();
    expect(client.getDeliveryIssue).toHaveBeenCalledWith("t1");
    answer(issue());

    expect(
      await screen.findByRole("heading", { name: "AGOS-455 Exportar facturas" }),
    ).toBeInTheDocument();
    for (const [label, value] of [
      ["Tipo", "Historia"],
      ["Estado", "En ejecución"],
      ["Prioridad", "Alta"],
      ["Asignada a", "Ana"],
      ["Etiquetas", "facturas, csv"],
      ["Padre", "AGOS-400 Facturación"],
    ]) {
      expect(screen.getByText(label).nextElementSibling).toHaveTextContent(value);
    }
    expect(screen.queryByText("Informador")).toBeNull();
    expect(screen.getByRole("list", { name: "Incidencias enlazadas" })).toHaveTextContent(
      "is blocked by AGOS-12 API Cerrada",
    );
    expect(screen.getByText("CSV").tagName).toBe("B");
    expect(screen.getByText("Hecho")).toBeInTheDocument();
    expect(screen.getByText("Luis")).toBeInTheDocument();

    // Links in Jira's HTML open on Jira's site, in the system's browser.
    await userEvent.click(screen.getByRole("link", { name: "AGOS-12" }));
    expect(client.openDeliveryUrl).toHaveBeenCalledWith("https://jira.test/browse/AGOS-12");
    await userEvent.click(screen.getByRole("link", { name: "Abrir en Jira ↗" }));
    expect(client.openDeliveryUrl).toHaveBeenCalledWith(URL);
  });

  it("says why it could not read the issue, and keeps the link to its page", async () => {
    client.getDeliveryIssue.mockRejectedValue({
      category: "jira_failed",
      message: "Jira respondió 401 Unauthorized",
    });
    render(<JiraIssue taskId="t1" issueUrl={URL} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo leer la incidencia: Jira respondió 401 Unauthorized",
    );
    expect(screen.getByRole("link", { name: "Abrir en Jira ↗" })).toHaveAttribute("href", URL);
  });

  it("shows Jira's HTML without scripts, handlers or embedded pages", async () => {
    client.getDeliveryIssue.mockResolvedValue(
      issue({
        description_html:
          '<p onclick="steal()" style="position:fixed">Texto</p><script>steal()</script>' +
          '<iframe src="https://evil.test"></iframe><img src="x" onerror="steal()">' +
          '<a href="javascript:steal()">malo</a>',
      }),
    );
    const { container } = render(<JiraIssue taskId="t1" issueUrl={URL} />);
    const text = await screen.findByText("Texto");
    expect(text).not.toHaveAttribute("onclick");
    expect(text).not.toHaveAttribute("style");
    expect(container.querySelector("script, iframe, [onerror]")).toBeNull();
    expect(container.querySelector("img")).toHaveAttribute("src", "https://jira.test/browse/x");
    expect(screen.getByText("malo")).not.toHaveAttribute("href");
    await userEvent.click(screen.getByText("malo"));
    expect(client.openDeliveryUrl).not.toHaveBeenCalled();
  });
});

describe("sanitizeIssueHtml", () => {
  it("keeps formatting and makes Jira's addresses absolute", () => {
    expect(
      sanitizeIssueHtml(
        '<p><em>a</em> <a href="/browse/X-1" onmouseover="x()">X-1</a> <a href="mailto:a@b.c">m</a></p>' +
          '<object data="x"></object><form><input></form><svg onload="x()"></svg>',
        "https://jira.test/browse/X-2",
      ),
    ).toBe(
      '<p><em>a</em> <a href="https://jira.test/browse/X-1">X-1</a> <a href="mailto:a@b.c">m</a></p>',
    );
  });
});
