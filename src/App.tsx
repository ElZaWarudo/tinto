import { DockWorkspace, type PanelComponents } from "./workspace/DockWorkspace";
import { PANEL_DASHBOARD, PANEL_TREE } from "./workspace/panels";
import "./App.css";

// Placeholder panels for the dock-shell gate (U2). Real panels (Dashboard,
// Repo, Repo tree) replace these in later units.
const placeholder = (label: string) =>
  function Placeholder() {
    return <div className="panel-placeholder">{label} (coming soon)</div>;
  };

const components: PanelComponents = {
  [PANEL_DASHBOARD]: placeholder("Dashboard"),
  [PANEL_TREE]: placeholder("Repos"),
};

export default function App() {
  return (
    <div className="app-shell">
      <DockWorkspace components={components} />
    </div>
  );
}
