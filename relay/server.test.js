import { test } from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import WebSocket from "ws";
import { MAX_FRAME, RATE_FRAMES, startRelay } from "./server.js";

const ROOM = "AAAAAAAAAAAAAAAAAAAAAA";

async function withRelay(run) {
  const relay = startRelay({ port: 0, host: "127.0.0.1" });
  await once(relay.http, "listening");
  const base = `ws://127.0.0.1:${relay.http.address().port}`;
  try {
    await run(base, relay);
  } finally {
    await relay.close();
  }
}

async function connect(base, role, room = ROOM) {
  const ws = new WebSocket(`${base}/v1/rooms/${room}?role=${role}`);
  await once(ws, "open");
  return ws;
}

const closeCode = async (ws) => (await once(ws, "close"))[0];

test("forwards binary frames between the two peers of a room, untouched", () =>
  withRelay(async (base) => {
    const desktop = await connect(base, "desktop");
    const phone = await connect(base, "phone");
    const frame = Buffer.from([1, 2, 3, 250]);
    phone.send(frame);
    const [received, isBinary] = await once(desktop, "message");
    assert.equal(isBinary, true);
    assert.deepEqual(Buffer.from(received), frame);
    desktop.send(Buffer.from("x"));
    assert.equal(Buffer.from((await once(phone, "message"))[0]).toString(), "x");
    desktop.close();
    phone.close();
  }));

test("refuses bad rooms and roles, text frames, oversized frames and floods", () =>
  withRelay(async (base, relay) => {
    for (const path of [`/v1/rooms/short?role=phone`, `/v1/rooms/${ROOM}?role=admin`]) {
      const ws = new WebSocket(base + path);
      await assert.rejects(once(ws, "open"));
    }

    const text = await connect(base, "phone");
    text.send("plain");
    assert.equal(await closeCode(text), 1003);

    const big = await connect(base, "phone");
    big.send(Buffer.alloc(MAX_FRAME + 1));
    // 1009, or 1006 when the relay drops the socket while it still sends.
    assert.ok([1006, 1009].includes(await closeCode(big)));

    const flood = await connect(base, "phone");
    for (let i = 0; i <= RATE_FRAMES; i += 1) flood.send(Buffer.from([i]));
    assert.equal(await closeCode(flood), 1008);

    // A new socket with the same role replaces the old one; empty rooms go.
    const first = await connect(base, "desktop");
    const second = await connect(base, "desktop");
    assert.equal(await closeCode(first), 4000);
    second.close();
    await once(second, "close");
    while (relay.rooms.size > 0) await new Promise((resolve) => setImmediate(resolve));
  }));

test("answers its health check", () =>
  withRelay(async (base) => {
    const response = await fetch(`${base.replace("ws", "http")}/healthz`);
    assert.equal(response.status, 200);
    assert.equal(await response.text(), "ok");
  }));
