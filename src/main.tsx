import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DetachedConsolesApp } from "./panels/terminal/DetachedConsolesApp";
import { DetachedTerminalApp } from "./panels/terminal/DetachedTerminalApp";
import {
  readDetachedConsolesFlag,
  readDetachedConsolesParams,
  readDetachedTerminalParams,
} from "./panels/terminal/detachTerminalWindow";

const detachedTerminalParams = readDetachedTerminalParams();
const detachedConsoles = readDetachedConsolesFlag();
const detachedConsolesParams = readDetachedConsolesParams();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {detachedConsoles ? (
      <DetachedConsolesApp initialTerminals={detachedConsolesParams} />
    ) : detachedTerminalParams ? (
      <DetachedTerminalApp params={detachedTerminalParams} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
