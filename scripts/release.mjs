// Cuts a release: `node scripts/release.mjs 1.2.0` (or `--kind patch|minor|major`,
// with `--rc` for a release candidate, to take the next version after the latest
// stable tag) sets the version in every manifest, adds its CHANGELOG.md section
// (from the conventional commits since the last tag, unless a hand-written one is
// already there), commits and tags v1.2.0. `--dry-run` prints that plan and
// changes nothing. Once the tag is pushed, the Release workflow builds and
// publishes it when you start it; the Release cut workflow does all three.
// `node scripts/release.mjs notes v1.2.0` prints that version's section, the
// release's notes on GitHub.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
const CHANGELOG = "CHANGELOG.md";
const GROUPS = [
  ["feat", "Features"],
  ["fix", "Fixes"],
  ["perf", "Performance"],
];

/** The changelog section of `version`, without its heading; null if none. */
export function sectionOf(changelog, version) {
  const lines = changelog.split(/\r?\n/);
  const start = lines.findIndex((line) => line.startsWith(`## [${version}]`));
  if (start < 0) return null;
  const end = lines.findIndex((line, at) => at > start && line.startsWith("## "));
  return lines
    .slice(start + 1, end < 0 ? undefined : end)
    .join("\n")
    .trim();
}

/** A changelog section from commit subjects: features, fixes and performance. */
export function sectionFrom(subjects, version, date) {
  const parts = [`## [${version}] - ${date}`];
  for (const [type, title] of GROUPS) {
    const items = subjects
      .map((subject) => subject.match(new RegExp(`^${type}(?:\\(([^)]+)\\))?!?: (.+)$`)))
      .filter(Boolean)
      .map(([, scope, text]) => `- ${scope ? `**${scope}:** ` : ""}${text}`);
    if (items.length > 0) parts.push(`### ${title}\n\n${items.join("\n")}`);
  }
  return parts.join("\n\n");
}

/** `changelog` with `section` as its newest entry, under the title. */
export function withSection(changelog, section) {
  const at = changelog.indexOf("\n## ");
  if (at < 0) return `${changelog.trimEnd()}\n\n${section}\n`;
  return `${changelog.slice(0, at).trimEnd()}\n\n${section}\n${changelog.slice(at)}`;
}

const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.]+))?$/;

/** Negative, zero or positive as semver `a` comes before, with or after `b`. */
export function compareVersions(a, b) {
  const [, ...x] = a.match(VERSION);
  const [, ...y] = b.match(VERSION);
  for (let at = 0; at < 3; at++) {
    if (Number(x[at]) !== Number(y[at])) return Number(x[at]) - Number(y[at]);
  }
  // A prerelease comes before its release; prereleases compare part by part.
  if (!x[3] || !y[3]) return (x[3] ? -1 : 0) + (y[3] ? 1 : 0);
  const p = x[3].split(".");
  const q = y[3].split(".");
  for (let at = 0; at < Math.min(p.length, q.length); at++) {
    if (p[at] === q[at]) continue;
    const numeric = /^\d+$/.test(p[at]) && /^\d+$/.test(q[at]);
    return numeric ? Number(p[at]) - Number(q[at]) : p[at] < q[at] ? -1 : 1;
  }
  return p.length - q.length;
}

/** The `kind` (patch, minor or major) after the latest stable tag; `-rc.N` for a candidate. */
export function nextVersion(tags, kind, rc = false) {
  const stable = tags
    .map((tag) => tag.replace(/^v/, ""))
    .filter((version) => VERSION.test(version) && !version.includes("-"))
    .sort(compareVersions)
    .at(-1);
  if (!stable) throw new Error("no stable vX.Y.Z tag to count from: pass the version");
  let [major, minor, patch] = stable.split(".").map(Number);
  if (kind === "major") [major, minor, patch] = [major + 1, 0, 0];
  else if (kind === "minor") [minor, patch] = [minor + 1, 0];
  else if (kind === "patch") patch += 1;
  else throw new Error(`«${kind}» is not patch, minor or major`);
  const base = `${major}.${minor}.${patch}`;
  if (!rc) return base;
  const candidates = tags
    .filter((tag) => tag.startsWith(`v${base}-rc.`))
    .map((tag) => Number(tag.slice(`v${base}-rc.`.length)))
    .filter(Number.isInteger);
  return `${base}-rc.${Math.max(0, ...candidates) + 1}`;
}

/** `text` with its first match of `pattern` (whose group 1 is the version) set to `version`. */
function setVersion(text, pattern, version, file) {
  const match = text.match(pattern);
  if (!match) throw new Error(`no version found in ${file}`);
  return text.replace(match[0], match[0].replace(match[1], version));
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

const VERSIONS = [
  ["package.json", /"version": "([^"]+)"/],
  ["package-lock.json", /"name": "tinto",\s*"version": "([^"]+)"/],
  ["package-lock.json", /"": \{\s*"name": "tinto",\s*"version": "([^"]+)"/],
  ["src-tauri/tauri.conf.json", /"version": "([^"]+)"/],
  ["src-tauri/Cargo.toml", /\[package\]\s*name = "tinto"\s*version = "([^"]+)"/],
  ["src-tauri/Cargo.lock", /name = "tinto"\s*version = "([^"]+)"/],
];

function cut(version, dryRun) {
  if (!VERSION.test(version)) {
    throw new Error(`«${version}» is not a version such as 1.2.0 or 1.2.0-beta.1`);
  }
  const tag = `v${version}`;
  const tags = git("tag", "--list", "v*").split("\n").filter(Boolean);
  if (tags.includes(tag)) throw new Error(`${tag} already exists`);
  const newest = [...tags.map((t) => t.slice(1)).filter((v) => VERSION.test(v)), appVersion()]
    .sort(compareVersions)
    .at(-1);
  if (compareVersions(version, newest) <= 0) {
    throw new Error(`${version} is not newer than ${newest}, the latest tag or app version`);
  }
  if (!dryRun && git("status", "--porcelain"))
    throw new Error("commit or stash your changes first");

  const files = new Map();
  for (const [file, pattern] of VERSIONS) {
    const text = files.get(file) ?? readFileSync(file, "utf8");
    files.set(file, setVersion(text, pattern, version, file));
  }

  const changelog = readFileSync(CHANGELOG, "utf8");
  let section = null;
  if (sectionOf(changelog, version) === null) {
    const last = git("tag", "--list", "v*", "--sort=-v:refname").split("\n")[0];
    if (!last)
      throw new Error(`no earlier tag: write the ${version} section of ${CHANGELOG} by hand`);
    const subjects = git("log", `${last}..HEAD`, "--format=%s").split("\n");
    const date = new Date().toISOString().slice(0, 10);
    section = sectionFrom(subjects, version, date);
    files.set(CHANGELOG, withSection(changelog, section));
  }

  if (dryRun) {
    console.log(`Would release ${tag} (app is ${appVersion()}), changing:`);
    for (const file of files.keys()) console.log(`  ${file}`);
    console.log(
      section === null
        ? `CHANGELOG.md already has a ${version} section; it is kept.`
        : `New CHANGELOG.md section:\n\n${section}\n`,
    );
    console.log(`Then: commit "chore(release): ${tag}" and tag ${tag}. Nothing was changed.`);
    return;
  }
  for (const [file, text] of files) writeFileSync(file, text);
  git("add", ...files.keys());
  git("commit", "-m", `chore(release): ${tag}`);
  git("tag", "-a", tag, "-m", `Tinto ${version}`);
  console.log(
    `Tagged ${tag}. Review CHANGELOG.md, then publish it with:\n  git push origin HEAD ${tag}\n  gh workflow run release.yml --ref develop -f tag=${tag}`,
  );
}

const appVersion = () => JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      kind: { type: "string" },
      rc: { type: "boolean", default: false },
      "dry-run": { type: "boolean", default: false },
    },
  });
  const [command, value] = positionals;
  if (command === "notes") {
    const notes = sectionOf(readFileSync(CHANGELOG, "utf8"), value.replace(/^v/, ""));
    if (notes === null) throw new Error(`${CHANGELOG} has no section for ${value}`);
    console.log(notes);
  } else {
    const version = values.kind
      ? nextVersion(git("tag", "--list", "v*").split("\n"), values.kind, values.rc)
      : command;
    cut(version, values["dry-run"]);
  }
}
