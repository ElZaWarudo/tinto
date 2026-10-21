import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DetachedConsolesApp } from "./panels/terminal/DetachedConsolesApp";
import { DetachedTerminalApp } from "./panels/terminal/DetachedTerminalApp";
import { DetachedPanelApp } from "./workspace/DetachedPanelApp";
import { themeStore } from "./qol/theme";
import { readDetachedPanel } from "./workspace/detachPanelWindow";
import {
  readDetachedConsolesFlag,
  readDetachedConsolesParams,
  readDetachedTerminalParams,
} from "./panels/terminal/detachTerminalWindow";

const detachedTerminalParams = readDetachedTerminalParams();
const detachedConsoles = readDetachedConsolesFlag();
const detachedConsolesParams = readDetachedConsolesParams();
const detachedPanel = readDetachedPanel();

// Every window (main and detached) starts in the saved theme and follows changes.
themeStore.hydrate();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {detachedPanel ? (
      <DetachedPanelApp panel={detachedPanel} />
    ) : detachedConsoles ? (
      <DetachedConsolesApp initialTerminals={detachedConsolesParams} />
    ) : detachedTerminalParams ? (
      <DetachedTerminalApp params={detachedTerminalParams} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
