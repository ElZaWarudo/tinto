import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface PingResponse {
  message: string;
  timestamp_ms: number;
}

interface TickPayload {
  timestamp_ms: number;
}

function App() {
  const [pingMessage, setPingMessage] = useState("contactando backend...");
  const [pingError, setPingError] = useState(false);
  const [lastTick, setLastTick] = useState<number | null>(null);

  useEffect(() => {
    let active = true;

    invoke<PingResponse>("ping")
      .then((res) => {
        if (active) setPingMessage(res.message);
      })
      .catch(() => {
        if (active) {
          setPingError(true);
          setPingMessage("error contactando el backend");
        }
      });

    const unlisten = listen<TickPayload>("tick", (event) => {
      if (active) setLastTick(event.payload.timestamp_ms);
    });

    return () => {
      active = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <main className="container">
      <h1>Tinto</h1>
      <p>Esqueleto del monitor de repos. Instrumentación de humo del puente webview↔Rust:</p>
      <p data-testid="ping" className={pingError ? "error" : undefined}>
        ping: {pingMessage}
      </p>
      <p data-testid="tick">
        tick: {lastTick === null ? "esperando..." : new Date(lastTick).toLocaleTimeString()}
      </p>
    </main>
  );
}

export default App;
