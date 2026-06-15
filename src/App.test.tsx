import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/react";

// The real dockable workspace is exercised in DockWorkspace.test.tsx; here we
// only assert App wires it up with the expected panel components.
const received: { components?: Record<string, unknown> } = {};
vi.mock("./workspace/DockWorkspace", () => ({
  DockWorkspace: (props: { components: Record<string, unknown> }) => {
    received.components = props.components;
    return <div data-testid="workspace" />;
  },
}));

import App from "./App";
import { PANEL_DASHBOARD, PANEL_TREE } from "./workspace/panels";

describe("App", () => {
  it("mounts the dockable workspace with the dashboard and tree panels", () => {
    const { getByTestId } = render(<App />);
    expect(getByTestId("workspace")).toBeInTheDocument();
    expect(Object.keys(received.components ?? {})).toEqual(
      expect.arrayContaining([PANEL_DASHBOARD, PANEL_TREE]),
    );
  });
});
