import React from "react";
import { createRoot } from "react-dom/client";
import "../App.css";
import { FileOverviewRuler, type FileOverviewMarker } from "../panels/file/FileOverviewRuler";

const lines = Array.from({ length: 80 }, (_, index) => {
  const line = index + 1;
  if ([7, 23, 38, 57, 76].includes(line)) return `const api_key_${line} = "demo";`;
  if ([13, 14, 46, 47].includes(line)) return `function changedLine${line}() { return ${line}; }`;
  return `const line${line} = "overview sample";`;
});

const markers: FileOverviewMarker[] = [
  ...[7, 23, 38, 57, 76].map((line) => ({
    line,
    severity: "critical" as const,
    label: "Possible secret",
    source: "alert" as const,
  })),
  ...[13, 14, 46, 47].map((line) => ({
    line,
    severity: "info" as const,
    label: "Changed line",
    source: "hunk" as const,
  })),
  ...[59, 73].map((line) => ({
    line,
    severity: "warning" as const,
    label: "Search result",
    source: "search" as const,
  })),
];

function Demo() {
  return (
    <main className="overview-demo">
      <section className="overview-demo__panel">
        <div className="file-view__body overview-demo__body">
          <div className="full-file" data-testid="full-file">
            <FileOverviewRuler
              markers={markers}
              totalLines={lines.length}
              targetAttribute="data-line"
            />
            <pre className="full-file__code">
              {lines.map((line, index) => (
                <div key={index} className="full-file__line" data-line={index + 1}>
                  <span className="diff-gutter">{index + 1}</span>
                  <code className="diff-content">{line}</code>
                </div>
              ))}
            </pre>
          </div>
        </div>
      </section>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Demo />
  </React.StrictMode>,
);
