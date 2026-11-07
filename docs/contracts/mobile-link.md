# Mobile link contract (v1)

The contract shared by the three parts of the mobile companion:

- **Tinto** (desktop, `src-tauri/src/mobile_link/`): owns the Delivery state and the keys.
- **Relay** (`relay/`): an untrusted, blind forwarder.
- **Phone** (`mobile/`, Flutter): shows a trimmed view of Delivery and sends a few allowlisted actions.

Everything below is normative. A change to it bumps the protocol version (`v`, the AAD and the relay path).

## 1. Pairing

1. The user sets the relay URL in Tinto (`wss://host[:port]`, or `ws://` for a local test). Empty means off: Tinto opens no connection.
2. "Vincular móvil" creates a **pairing**: a random 128-bit `roomId` and a random 256-bit `key`, both encoded base64url without padding (22 and 43 characters).
3. Tinto shows a QR code (and the same text to copy) with the link:

   ```
   tinto-link://<host>[:<port>]/<roomId>#<key>
   tinto-link://<host>[:<port>]/<roomId>?tls=0#<key>     (relay over ws://, tests only)
   ```

   The key travels only in the fragment of the QR. It is never sent to the relay.
4. Tinto connects to the room at once. The phone scans the QR (or pastes the link), connects, and sends `hello` with its device name.
5. Tinto shows the device name and the user confirms it once. Until then Tinto answers `hello` with `status: "pending"` and sends nothing else. An unconfirmed pairing expires after 10 minutes.
6. Once confirmed, the room and key are a **paired device**, stored in Tinto's Delivery store like the Jira token (never in the overview, never shown again). Tinto keeps one connection per paired device.
7. **Revoking** a device deletes its key and room from Tinto, sends it `revoked` if it is online, and closes the connection. Without the key nothing it sends can be read, so a revoked phone has to pair again.

Each device has its own room and key: revoking one leaves the others untouched.

## 2. Transport

- Both peers open `wss://<relay>/v1/rooms/<roomId>?role=desktop|phone`.
- A room holds at most one `desktop` and one `phone`. A second connection with the same role replaces the first (close code 4000).
- The relay forwards each **binary** frame from one peer to the other, unchanged. A frame sent while the other peer is absent is dropped. Text frames close the socket (1003).
- Limits: a frame is at most **256 KiB** (larger closes with 1009); at most **60 frames and 4 MiB per 10 s** per socket (more closes with 1008). The relay pings every 30 s and drops sockets that miss a pong. A room exists only while one of its sockets does.
- `GET /healthz` answers `200 ok`.

## 3. Encryption and framing

Each frame is `nonce (12 bytes) || ciphertext || tag (16 bytes)`:

- AEAD **ChaCha20-Poly1305** (RFC 8439), key = the pairing key, a **random 96-bit nonce** per frame, AAD = the ASCII bytes `tinto-link/1`.
- The plaintext is one UTF-8 JSON object, at most 256 KiB − 28 bytes.
- A frame that does not decrypt is dropped silently.

Random nonces are safe here: a pairing exchanges far fewer than 2³² frames.

### Envelope

Every plaintext carries:

| field  | meaning                                                                          |
| ------ | -------------------------------------------------------------------------------- |
| `v`    | `1`                                                                              |
| `from` | `desktop` or `phone`; a frame from one's own role is dropped (reflection)        |
| `type` | the message type                                                                 |
| `sid`  | the sender's session id: 128 random bits, base64url, new on every connection     |
| `peer` | the receiver's `sid` as the sender knows it, `""` before it knows it             |
| `seq`  | integer, starts at 1 per `sid` and increases by one per frame sent               |

### Freshness (replay rejection)

- `hello` is accepted unless its `seq` is not greater than the last seen for the same `sid`. A new `sid` becomes the current peer session.
- Every other message is accepted only if its `sid` is the current peer session, its `peer` equals the receiver's own `sid`, and its `seq` is greater than the last accepted one.
- When a `hello` arrives whose `peer` is not the receiver's `sid`, the receiver answers with its own `hello` (with `peer` set). So each side learns the other's session without the relay telling anyone who is present.

Because `sid` is fresh per connection and every non-hello message names the receiver's current `sid`, frames recorded in an earlier session (or earlier in this one) are refused.

## 4. Messages

### Phone → desktop

- `hello`: `{ "device": "<name, ≤ 64 chars>" }`.
- `action`: `{ "id": "<request id, ≤ 64 chars>", "action": { "action": "<name>", ... } }`. Only these, with exactly these fields:

| `action`             | fields                                                     | Tinto calls (as the user)                        |
| -------------------- | ---------------------------------------------------------- | ------------------------------------------------ |
| `answer_decision`    | `decision_id`, `answer`                                    | `answer_decision(id, answer, "user")`            |
| `accept_recommended` | `task_id`                                                  | `accept_recommended(task_id, "user")`            |
| `decide_approval`    | `approval_id`, `approve` (bool), `digest`                  | `decide_approval(id, approve, None)`             |
| `send_qa_problems`   | `task_id`, `choices: [{ "problem_id", "solution" }]`       | `send_qa_problems(task_id, choices)`             |
| `set_run_paused`     | `run_id`, `paused` (bool)                                  | `set_run_paused(run_id, paused)`                 |

  Anything else (an unknown action, an extra field) is refused with `error`. `decide_approval` with `approve: true` is refused unless `digest` equals the approval's current digest (§5), so the phone approves exactly the text it showed; an approval whose text did not fit (`truncated`) can only be approved in Tinto. The same service validation as in the desktop panel applies to all of them.

### Desktop → phone

- `hello`: `{ "status": "pending" | "paired" }`.
- `snapshot`: the trimmed view (§5). Sent after a `hello` from a paired phone and after every Delivery change, debounced (at most one per second).
- `ack`: `{ "id" }`, the action was done.
- `error`: `{ "id", "message" }`, it was refused or failed; `message` is in Spanish, for the user.
- `revoked`: `{}`, the device was revoked; the phone forgets the pairing.

## 5. Snapshot

```json
{
  "at_ms": 0,
  "runs": [{ "id", "title", "paused" }],
  "tasks": [{ "id", "run_id", "key", "title",
              "phase": { "kind", "label", "waits_on" },
              "pr": { "number", "url", "state", "draft", "conflicts", "ci", "review_decision" } | null }],
  "decisions": [{ "id", "task_id", "kind", "question", "detail", "text", "command",
                  "options": [{ "label", "consequence", "recommended" }],
                  "recommendation": { "why", "basis", "unknown" } | null, "blocking" }],
  "approvals": [{ "id", "task_id", "rung", "stage", "title", "body",
                  "commits": ["<message>"], "merge_method", "jira_statuses", "reviewers",
                  "requested_at_ms", "digest", "truncated" }],
  "qa_problems": [{ "id", "task_id", "number", "label", "place", "text", "suggestions" }],
  "settings_proposals": [{ "id", "task_id", "reason" }],
  "failures": [{ "task_id", "role", "error", "ended_at_ms" }]
}
```

- Only active batches, their live tasks, and what is **pending**: decisions, approvals, open QA problems, settings proposals (read only on the phone).
- `phase.label` is Spanish, ready to show; `waits_on` is Tinto's `user`, `agent`, `coordinator`… A task waits on the user when `waits_on == "user"`.
- `failures`: the 5 latest failed jobs, newest first, `error` cut to 500 characters.
- `digest`: lowercase hex SHA-256 of the approval's fields as sent (before cutting). Opaque to the phone.
- `truncated`: some text of the approval was longer than 16 000 characters and was cut.
- Strings other than approval texts are cut to 4 000 characters. If the snapshot is still too large, `failures` and decision `detail` are dropped.

Not sent, ever: file contents, diffs, logs, prompts, paths, tokens, settings. The only code the phone shows is what a decision already carries (`text`, `command`).

## 6. Threat model

- **Relay**: sees room ids, roles, connection times, IPs, frame sizes and timing. It cannot read or forge messages (AEAD), replay them (§3 freshness), reflect them (`from`), or learn the key (QR fragment only). It can drop, delay or refuse traffic, and anyone who learns a room id can kick a peer (replace) — denial of service only.
- **Network**: TLS to the relay hides room ids from observers; contents are end-to-end encrypted even over `ws://`.
- **Lost phone**: whoever holds it can act as the user until the device is revoked in Tinto. The key is kept in the platform's secure storage.
- **Desktop**: keys are stored in Tinto's Delivery store in the config directory, with the same protection as the Jira token (the user's account).
- The phone can only call the five actions above, with the desktop's own validation. It cannot run commands, read files or change settings.

## 7. Shared test vector

Both implementations check that sealing this plaintext with this key and nonce gives this frame, and that opening it gives the plaintext back.

```
key (base64url)  AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8      (bytes 0x00..0x1f)
nonce (hex)      404142434445464748494a4b
aad              tinto-link/1
plaintext        {"v":1,"from":"phone","type":"hello","sid":"c2lkLXBob25lLTAwMDAwMQ","peer":"","seq":1,"device":"Prueba"}
frame (hex)      404142434445464748494a4b83760aa3487bc2233ebd996d99d54fbddca30d7d66d0631b877cb779d005520f9cdff6520a69119f6e4d1565096682f5ad0b2ea69c44b84397964e4c452f721686610b717163824e302c57abe1020719b82ab214cd91c20ad807c54e697eb856ce4afb3f4ee5370a59138dded052b70cb1ac0871a1bda69c
```

(Computed independently with Node's OpenSSL `chacha20-poly1305`.)
