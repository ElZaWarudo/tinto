import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DetachedTerminalApp } from "./panels/terminal/DetachedTerminalApp";
import { readDetachedTerminalParams } from "./panels/terminal/detachTerminalWindow";

const detachedTerminalParams = readDetachedTerminalParams();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {detachedTerminalParams ? <DetachedTerminalApp params={detachedTerminalParams} /> : <App />}
  </React.StrictMode>,
);
