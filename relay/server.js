// Blind relay between Tinto and its phone (docs/contracts/mobile-link.md):
// a room holds one desktop and one phone, and each binary frame goes from
// one to the other unchanged. Frames are end-to-end encrypted; the relay
// keeps nothing but the live sockets.

import { createServer } from "node:http";
import { pathToFileURL } from "node:url";
import { WebSocket, WebSocketServer } from "ws";

export const MAX_FRAME = 256 * 1024;
export const RATE_WINDOW_MS = 10_000;
export const RATE_FRAMES = 60;
export const RATE_BYTES = 4 * 1024 * 1024;
const PING_MS = 30_000;
const ROOM_PATH = /^\/v1\/rooms\/([A-Za-z0-9_-]{22})$/;
const ROLES = ["desktop", "phone"];

export function startRelay({ port = 8787, host } = {}) {
  /** roomId -> { desktop?: WebSocket, phone?: WebSocket } */
  const rooms = new Map();

  const http = createServer((req, res) => {
    if (req.method === "GET" && req.url === "/healthz") {
      res.writeHead(200, { "content-type": "text/plain" }).end("ok");
      return;
    }
    res.writeHead(404).end();
  });

  const wss = new WebSocketServer({ noServer: true, maxPayload: MAX_FRAME });

  http.on("upgrade", (req, socket, head) => {
    const url = new URL(req.url ?? "/", "http://relay");
    const room = ROOM_PATH.exec(url.pathname)?.[1];
    const role = url.searchParams.get("role");
    if (!room || !ROLES.includes(role)) {
      socket.end("HTTP/1.1 400 Bad Request\r\n\r\n");
      return;
    }
    wss.handleUpgrade(req, socket, head, (ws) => join(ws, room, role));
  });

  function join(ws, roomId, role) {
    const room = rooms.get(roomId) ?? {};
    rooms.set(roomId, room);
    room[role]?.close(4000, "replaced");
    room[role] = ws;
    const other = role === "desktop" ? "phone" : "desktop";
    let windowStart = Date.now();
    let frames = 0;
    let bytes = 0;
    ws.alive = true;

    ws.on("pong", () => (ws.alive = true));
    ws.on("message", (data, isBinary) => {
      if (!isBinary) {
        ws.close(1003, "binary frames only");
        return;
      }
      const now = Date.now();
      if (now - windowStart >= RATE_WINDOW_MS) {
        windowStart = now;
        frames = 0;
        bytes = 0;
      }
      frames += 1;
      bytes += data.length;
      if (frames > RATE_FRAMES || bytes > RATE_BYTES) {
        ws.close(1008, "rate limit");
        return;
      }
      const peer = room[other];
      if (peer?.readyState === WebSocket.OPEN) peer.send(data, { binary: true });
    });
    ws.on("close", () => {
      if (room[role] === ws) delete room[role];
      if (!room.desktop && !room.phone && rooms.get(roomId) === room) rooms.delete(roomId);
    });
    ws.on("error", () => ws.terminate());
  }

  // Sockets that stop answering pings are dropped, and with them their room.
  const heartbeat = setInterval(() => {
    for (const ws of wss.clients) {
      if (!ws.alive) {
        ws.terminate();
        continue;
      }
      ws.alive = false;
      ws.ping();
    }
  }, PING_MS);

  http.listen(port, host);
  return {
    http,
    rooms,
    close: () =>
      new Promise((resolve) => {
        clearInterval(heartbeat);
        for (const ws of wss.clients) ws.terminate();
        http.close(() => resolve());
      }),
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const port = Number(process.env.PORT ?? 8787);
  startRelay({ port });
  console.log(`tinto-relay listening on :${port}`);
}
