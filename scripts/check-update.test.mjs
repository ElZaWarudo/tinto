import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { expect, test } from "vitest";

import { updateProblems } from "./check-update.mjs";

// A minisign key and signatures as Tauri writes them, from a throwaway key.
const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const keyId = Buffer.from("0123456789abcdef", "hex");
const raw = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
const b64 = (text) => Buffer.from(text).toString("base64");
const pubkey = b64(
  `untrusted comment: minisign public key\n${Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64")}\n`,
);
function signatureOf(data) {
  const sig = sign(null, createHash("blake2b512").update(data).digest(), privateKey);
  const comment = "timestamp:1\tfile:Tinto_1.3.0_x64-setup.exe";
  const global = sign(null, Buffer.concat([sig, Buffer.from(comment)]), privateKey);
  const line = Buffer.concat([Buffer.from("ED"), keyId, sig]).toString("base64");
  return b64(
    `untrusted comment: x\n${line}\ntrusted comment: ${comment}\n${global.toString("base64")}\n`,
  );
}

const installer = Buffer.from("the installer");
const url = "https://api.github.com/repos/me/tinto/releases/assets/7";
const check = (manifest, files = { "Tinto_1.3.0_x64-setup.exe": installer }) =>
  updateProblems({
    manifest,
    previous: { version: "1.2.2" },
    tag: "v1.3.0",
    repo: "me/tinto",
    assets: [{ name: "Tinto_1.3.0_x64-setup.exe", apiUrl: url, url: "https://github.com/x" }],
    config: {
      plugins: {
        updater: {
          pubkey,
          endpoints: ["https://github.com/me/tinto/releases/latest/download/latest.json"],
        },
      },
    },
    read: (name) => files[name],
  });
const windows = { url, signature: signatureOf(installer) };

test("a signed update of this release's installer checks out", () => {
  expect(check({ version: "1.3.0", platforms: { "windows-x86_64": windows } })).toEqual([]);
});

test("each way an update breaks is reported", () => {
  expect(
    check(
      {
        version: "1.3.0",
        platforms: {
          "windows-x86_64-nsis": { ...windows, url: "https://example.com/old.exe" },
          "windows-x86_64-msi": windows,
        },
      },
      { "Tinto_1.3.0_x64-setup.exe": Buffer.from("another build") },
    ),
  ).toEqual([
    "latest.json has no windows-x86_64",
    "windows-x86_64-nsis: https://example.com/old.exe is not an asset of v1.3.0",
    "windows-x86_64-msi: Tinto_1.3.0_x64-setup.exe: signature does not match the file",
  ]);
  expect(check({ version: "1.2.2", platforms: {} })).toContain(
    "latest.json is version 1.2.2, the tag v1.3.0",
  );
});
