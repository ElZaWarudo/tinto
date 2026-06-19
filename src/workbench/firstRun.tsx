// First-run empty state (R8): no workbench yet → create one inline. After
// creation the app shows the workspace (zero-repos state prompts adding repos).

import { useState } from "react";
import tintoWordmarkDark from "../assets/brand/tinto-wordmark-dark.png";
import { createAndActivate } from "./operations";

export function FirstRun() {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await createAndActivate(name);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="first-run" data-testid="first-run">
      <div className="first-run__brand">
        <img src={tintoWordmarkDark} alt="Tinto" />
      </div>
      <h1>Welcome to Tinto</h1>
      <p>Create a workbench to start monitoring repositories.</p>
      <div className="first-run__form">
        <input
          data-testid="wb-name"
          placeholder="Workbench name (e.g. Work)"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void create()}
        />
        <button data-testid="create-wb" onClick={() => void create()} disabled={busy}>
          Create
        </button>
      </div>
    </div>
  );
}
