// Cuts a release: `node scripts/release.mjs 1.2.0` sets the version in every
// manifest, adds its CHANGELOG.md section (from the conventional commits since
// the last tag, unless a hand-written one is already there), commits and tags
// v1.2.0. Pushing the tag makes the Release workflow build and publish it.
// `node scripts/release.mjs notes v1.2.0` prints that version's section, the
// release's notes on GitHub.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

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

/** `text` with its first match of `pattern` (whose group 1 is the version) set to `version`. */
function setVersion(text, pattern, version, file) {
  const match = text.match(pattern);
  if (!match) throw new Error(`no version found in ${file}`);
  return text.replace(match[0], match[0].replace(match[1], version));
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

function cut(version) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/.test(version)) {
    throw new Error(`«${version}» is not a version such as 1.2.0 or 1.2.0-beta.1`);
  }
  const tag = `v${version}`;
  if (git("status", "--porcelain")) throw new Error("commit or stash your changes first");
  if (git("tag", "--list", tag)) throw new Error(`${tag} already exists`);

  const versions = [
    ["package.json", /"version": "([^"]+)"/],
    ["package-lock.json", /"name": "tinto",\s*"version": "([^"]+)"/],
    ["package-lock.json", /"": \{\s*"name": "tinto",\s*"version": "([^"]+)"/],
    ["src-tauri/tauri.conf.json", /"version": "([^"]+)"/],
    ["src-tauri/Cargo.toml", /\[package\]\s*name = "tinto"\s*version = "([^"]+)"/],
    ["src-tauri/Cargo.lock", /name = "tinto"\s*version = "([^"]+)"/],
  ];
  for (const [file, pattern] of versions) {
    writeFileSync(file, setVersion(readFileSync(file, "utf8"), pattern, version, file));
  }

  let changelog = readFileSync(CHANGELOG, "utf8");
  if (sectionOf(changelog, version) === null) {
    const last = git("tag", "--list", "v*", "--sort=-v:refname").split("\n")[0];
    if (!last)
      throw new Error(`no earlier tag: write the ${version} section of ${CHANGELOG} by hand`);
    const subjects = git("log", `${last}..HEAD`, "--format=%s").split("\n");
    const date = new Date().toISOString().slice(0, 10);
    changelog = withSection(changelog, sectionFrom(subjects, version, date));
    writeFileSync(CHANGELOG, changelog);
  }

  git("add", CHANGELOG, ...new Set(versions.map(([file]) => file)));
  git("commit", "-m", `chore(release): ${tag}`);
  git("tag", "-a", tag, "-m", `Tinto ${version}`);
  console.log(
    `Tagged ${tag}. Review CHANGELOG.md, then publish it with:\n  git push origin HEAD ${tag}`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [command, value] = process.argv.slice(2);
  if (command === "notes") {
    const notes = sectionOf(readFileSync(CHANGELOG, "utf8"), value.replace(/^v/, ""));
    if (notes === null) throw new Error(`${CHANGELOG} has no section for ${value}`);
    console.log(notes);
  } else {
    cut(command);
  }
}
