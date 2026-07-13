// RUL-001 rail demo fixture. Mounts the FileOverviewRuler against a mock
// file body with 80 lines. The fixture runs entirely in the browser with no
// Tauri backend, so the rail, scroll-synced caret, click handlers, and keyboard
// nav can be reviewed via:
//   npm run dev
//   http://127.0.0.1:1420/demo.html

import React, { type CSSProperties } from "react";
import { createRoot } from "react-dom/client";
import { useEffect, useRef, useState } from "react";
import { FileOverviewRuler, type FileOverviewMarker } from "../panels/file/FileOverviewRuler";
import { useOverviewScrollSync } from "../panels/file/useOverviewScrollSync";
import "../App.css";
import "./demo.css";

const SAMPLE_LINES: { line: number; text: string; isSecret: boolean }[] = [
  { line: 1, text: "import { sign } from 'jsonwebtoken';", isSecret: false },
  { line: 2, text: "import { readFile } from 'fs/promises';", isSecret: false },
  { line: 3, text: "", isSecret: false },
  { line: 4, text: "const config = {", isSecret: false },
  { line: 5, text: "  port: 3000,", isSecret: false },
  { line: 6, text: "  host: 'localhost',", isSecret: false },
  { line: 7, text: "  api_key: 'sk_live_4eC39HqLyjWDarjtT1zdp7dc',", isSecret: true },
  { line: 8, text: "  debug: false,", isSecret: false },
  { line: 9, text: "};", isSecret: false },
  { line: 10, text: "", isSecret: false },
  { line: 11, text: "function loadSecrets() {", isSecret: false },
  { line: 12, text: "  return {", isSecret: false },
  { line: 13, text: "    db_password: 'correcthorsebatterystaple',", isSecret: true },
  { line: 14, text: "    redis_url: 'redis://default:abc123@host:6379',", isSecret: true },
  { line: 15, text: "  };", isSecret: false },
  { line: 16, text: "}", isSecret: false },
  { line: 17, text: "", isSecret: false },
  { line: 18, text: "async function handler(req, res) {", isSecret: false },
  { line: 19, text: "  const token = req.headers['authorization'];", isSecret: false },
  { line: 20, text: "  if (!token) return res.status(401).end();", isSecret: false },
  { line: 21, text: "", isSecret: false },
  { line: 22, text: "  // OAuth setup", isSecret: false },
  { line: 23, text: "  const client_secret = process.env.CLIENT_SECRET;", isSecret: true },
  { line: 24, text: "  const access_token = await exchange(client_secret);", isSecret: true },
  { line: 25, text: "  const refresh_token = await rotate(access_token);", isSecret: true },
  { line: 26, text: "  res.json({ access_token, refresh_token });", isSecret: true },
  { line: 27, text: "}", isSecret: false },
  { line: 28, text: "", isSecret: false },
  { line: 29, text: "function start() {", isSecret: false },
  { line: 30, text: "  const server = http.createServer(handler);", isSecret: false },
  { line: 31, text: "  server.listen(config.port, config.host, () => {", isSecret: false },
  { line: 32, text: "    console.log('listening on ' + config.port);", isSecret: false },
  { line: 33, text: "  });", isSecret: false },
  { line: 34, text: "}", isSecret: false },
  { line: 35, text: "", isSecret: false },
  { line: 36, text: "// --- middleware ---", isSecret: false },
  { line: 37, text: "function auth(req, res, next) {", isSecret: false },
  { line: 38, text: "  const auth_token = req.cookies.auth;", isSecret: true },
  { line: 39, text: "  if (!auth_token) return res.status(401).end();", isSecret: true },
  { line: 40, text: "  next();", isSecret: false },
  { line: 41, text: "}", isSecret: false },
  { line: 42, text: "", isSecret: false },
  { line: 43, text: "function login(req, res) {", isSecret: false },
  { line: 44, text: "  const username = req.body.username;", isSecret: false },
  { line: 45, text: "  const password = req.body.password;", isSecret: false },
  {
    line: 46,
    text: "  const session_token = crypto.randomBytes(32).toString('hex');",
    isSecret: true,
  },
  { line: 47, text: "  res.cookie('auth', session_token);", isSecret: true },
  { line: 48, text: "  res.json({ ok: true });", isSecret: false },
  { line: 49, text: "}", isSecret: false },
  { line: 50, text: "", isSecret: false },
  { line: 51, text: "function logout(req, res) {", isSecret: false },
  { line: 52, text: "  res.clearCookie('auth');", isSecret: false },
  { line: 53, text: "  res.json({ ok: true });", isSecret: false },
  { line: 54, text: "}", isSecret: false },
  { line: 55, text: "", isSecret: false },
  { line: 56, text: "// --- infra ---", isSecret: false },
  { line: 57, text: "const AWS_ACCESS_KEY_ID = 'AKIAIOSFODNN7EXAMPLE';", isSecret: true },
  {
    line: 58,
    text: "const AWS_SECRET_ACCESS_KEY = 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY';",
    isSecret: true,
  },
  { line: 59, text: "const private_key = `-----BEGIN PRIVATE KEY-----", isSecret: true },
  { line: 60, text: "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQ...`;", isSecret: false },
  { line: 61, text: "", isSecret: false },
  { line: 62, text: "function boot() {", isSecret: false },
  { line: 63, text: "  loadSecrets();", isSecret: false },
  { line: 64, text: "  start();", isSecret: false },
  { line: 65, text: "}", isSecret: false },
  { line: 66, text: "", isSecret: false },
  { line: 67, text: "if (require.main === module) {", isSecret: false },
  { line: 68, text: "  boot();", isSecret: false },
  { line: 69, text: "}", isSecret: false },
  { line: 70, text: "", isSecret: false },
  { line: 71, text: "// --- helpers ---", isSecret: false },
  { line: 72, text: "async function exchange(secret) {", isSecret: false },
  { line: 73, text: "  return 'eyJhbGciOiJIUzI1NiJ9.payload.signature';", isSecret: true },
  { line: 74, text: "}", isSecret: false },
  { line: 75, text: "async function rotate(token) {", isSecret: false },
  { line: 76, text: "  return 'refresh_' + Math.random().toString(36).slice(2);", isSecret: true },
  { line: 77, text: "}", isSecret: false },
  { line: 78, text: "", isSecret: false },
  { line: 79, text: "export { config, handler, auth, login, logout };", isSecret: false },
  { line: 80, text: "", isSecret: false },
];

const MARKERS: FileOverviewMarker[] = SAMPLE_LINES.filter((line) => line.isSecret).map((line) => ({
  line: line.line,
  severity: "critical" as const,
  label: "Posible secreto",
}));

export function DemoFileView() {
  const bodyRef = useRef<HTMLDivElement>(null);
  const totalLines = SAMPLE_LINES.length;
  const { topLine, visibleLineCount, viewportHeight, scrollProgress } = useOverviewScrollSync(
    bodyRef,
    totalLines,
  );
  const [activeLine, setActiveLine] = useState<number | null>(null);
  const scrollPastEndStyle =
    viewportHeight > 0
      ? ({
          "--file-scroll-past-end": `${Math.max(0, viewportHeight - 18)}px`,
        } as CSSProperties)
      : undefined;

  const handleJump = (line: number) => {
    const root = bodyRef.current ?? document;
    const el = root.querySelector(`[data-line="${line}"]`) as HTMLElement | null;
    el?.scrollIntoView({ block: "center", inline: "nearest" });
    setActiveLine(line);
  };

  useEffect(() => {
    if (activeLine == null) return;
    if (topLine <= activeLine) return;

    const clearTimer = window.setTimeout(() => setActiveLine(null), 0);
    return () => window.clearTimeout(clearTimer);
  }, [topLine, activeLine]);

  return (
    <div className="demo">
      <header className="demo__header">
        <h1>Minimapa de archivo RUL-001 — muestra de desarrollo</h1>
        <p>
          Archivo de muestra: <code>src/handler.ts</code> — 80 líneas — {MARKERS.length} avisos de
          posible secreto. Abre las herramientas de desarrollo (F12) para ver la consola.
        </p>
        <p className="demo__hints">
          <strong>Prueba:</strong> desplaza el contenido — pulsa el minimapa — pulsa una marca roja
          — usa Tab y las flechas — Inicio / Fin
        </p>
      </header>
      <div className="file-view" data-testid="demo-file-view">
        <div className="file-view__toolbar">
          <span className="file-view__path" title="demo - src/handler.ts">
            src/handler.ts
          </span>
          <span className="file-view__mode">Muestra de desarrollo</span>
        </div>
        <div className="file-view__body" ref={bodyRef} data-testid="demo-body">
          <FileOverviewRuler
            markers={MARKERS}
            totalLines={totalLines}
            topLine={topLine}
            visibleLineCount={visibleLineCount}
            scrollProgress={scrollProgress}
            overviewLines={SAMPLE_LINES.map((line) => line.text)}
            bodyRef={bodyRef}
            targetAttribute="data-line"
            onActiveLineChange={setActiveLine}
          />
          <pre className="full-file__code" style={scrollPastEndStyle}>
            {SAMPLE_LINES.map((line) => (
              <div
                key={line.line}
                className={`full-file__line${
                  line.isSecret ? " full-file__line--signal-critical" : ""
                }${activeLine === line.line ? " full-file__line--active" : ""}`}
                data-line={line.line}
                onClick={() => handleJump(line.line)}
              >
                <span className="diff-gutter">{String(line.line).padStart(3, " ")}</span>
                <code className="diff-content">{line.text || " "}</code>
              </div>
            ))}
          </pre>
        </div>
      </div>
      <aside className="demo__aside">
        <h2>Estado</h2>
        <ul>
          <li>
            <strong>topLine:</strong> {topLine}
          </li>
          <li>
            <strong>activeLine:</strong> {activeLine ?? "-"}
          </li>
          <li>
            <strong>markers:</strong> {MARKERS.length}
          </li>
        </ul>
        <p className="demo__aside-note">
          topLine cambia al desplazar el contenido. activeLine cambia al pulsar una marca o el
          minimapa.
        </p>
      </aside>
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <React.StrictMode>
      <DemoFileView />
    </React.StrictMode>,
  );
}
