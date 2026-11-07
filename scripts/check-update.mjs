// Checks a release's update before it is published, the way an installed Tinto
// will read it: `latest.json` is at the URL the app asks, names this version
// (newer than the one installed Tinto runs), has the Windows platform, points at
// assets of this release, and each signature verifies against the app's
// updater public key over those exact bytes. The Release workflow runs it on
// the draft release:
//   node scripts/check-update.mjs --tag v1.2.3 --repo owner/tinto \
//     --dir <downloaded assets> --assets <gh release view --json assets> \
//     --previous <the published release's latest.json>
import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { compareVersions } from "./release.mjs";

const lines = (base64) => Buffer.from(base64, "base64").toString("utf8").trim().split(/\r?\n/);

/**
 * Verifies a minisign `signature` (as Tauri writes it: the .sig file, base64)
 * of `data` with `pubkey` (tauri.conf.json's updater pubkey). Throws if it does not.
 */
export function verifySignature(pubkey, signature, data) {
  const key = Buffer.from(lines(pubkey)[1], "base64"); // "Ed", key id (8), key (32)
  const [, sigLine, trusted, globalLine] = lines(signature);
  const sig = Buffer.from(sigLine, "base64"); // "ED" or "Ed", key id (8), signature (64)
  if (!sig.subarray(2, 10).equals(key.subarray(2, 10))) {
    throw new Error("signed with another key than the app's updater pubkey");
  }
  const publicKey = createPublicKey({
    key: { kty: "OKP", crv: "Ed25519", x: key.subarray(10).toString("base64url") },
    format: "jwk",
  });
  const algorithm = sig.subarray(0, 2).toString();
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, publicKey, sig.subarray(10))) {
    throw new Error("signature does not match the file");
  }
  const comment = Buffer.from(trusted.replace(/^trusted comment: /, ""));
  const global = Buffer.from(globalLine, "base64");
  if (!verify(null, Buffer.concat([sig.subarray(10), comment]), publicKey, global)) {
    throw new Error("trusted comment signature does not verify");
  }
}

/** What would break the update; empty when it is fine. */
export function updateProblems({ manifest, previous, tag, repo, assets, config, read }) {
  const problems = [];
  const endpoint = `https://github.com/${repo}/releases/latest/download/latest.json`;
  if (!config.plugins.updater.endpoints.includes(endpoint)) {
    problems.push(`tauri.conf.json does not ask ${endpoint}`);
  }
  if (manifest.version?.replace(/^v/, "") !== tag.replace(/^v/, "")) {
    problems.push(`latest.json is version ${manifest.version}, the tag ${tag}`);
  } else if (previous && compareVersions(manifest.version, previous.version) <= 0) {
    problems.push(`${manifest.version} is not newer than the published ${previous.version}`);
  }
  const platforms = Object.entries(manifest.platforms ?? {});
  if (!manifest.platforms?.["windows-x86_64"]) problems.push("latest.json has no windows-x86_64");
  for (const [platform, { url, signature }] of platforms) {
    const asset = assets.find((a) => a.apiUrl === url || a.url === url);
    if (!asset) {
      problems.push(`${platform}: ${url} is not an asset of ${tag}`);
      continue;
    }
    try {
      verifySignature(config.plugins.updater.pubkey, signature, read(asset.name));
    } catch (error) {
      problems.push(`${platform}: ${asset.name}: ${error.message}`);
    }
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({
    options: Object.fromEntries(
      ["tag", "repo", "dir", "assets", "previous"].map((name) => [name, { type: "string" }]),
    ),
  });
  const json = (file) => JSON.parse(readFileSync(file, "utf8"));
  const manifest = json(join(values.dir, "latest.json"));
  const problems = updateProblems({
    manifest,
    previous: values.previous ? json(values.previous) : null,
    tag: values.tag,
    repo: values.repo,
    assets: json(values.assets),
    config: json("src-tauri/tauri.conf.json"),
    read: (name) => readFileSync(join(values.dir, name)),
  });
  if (problems.length > 0) {
    console.error(`The ${values.tag} update would not work:\n- ${problems.join("\n- ")}`);
    process.exit(1);
  }
  console.log(`The ${values.tag} update checks out: ${Object.keys(manifest.platforms).join(", ")}`);
}
