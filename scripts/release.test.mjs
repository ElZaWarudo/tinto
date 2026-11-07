import { expect, test } from "vitest";

import { nextVersion, sectionFrom, sectionOf, withSection } from "./release.mjs";

test("groups conventional commits into the version's section and skips the rest", () => {
  const section = sectionFrom(
    [
      "feat(delivery): run only the checks a stage reaches",
      "fix: an empty bootstrap removes it",
      "perf(agents)!: read the journal once",
      "chore(release): v1.0.0",
      "docs: explain releases",
    ],
    "1.1.0",
    "2026-11-02",
  );
  expect(section).toBe(
    [
      "## [1.1.0] - 2026-11-02",
      "### Features\n\n- **delivery:** run only the checks a stage reaches",
      "### Fixes\n\n- an empty bootstrap removes it",
      "### Performance\n\n- **agents:** read the journal once",
    ].join("\n\n"),
  );
});

test("a new section goes on top, and each version's notes stop at the next one", () => {
  const changelog = withSection(
    "# Changelog\n\n## [1.0.0] - 2026-10-29\n\nFirst stable release.\n",
    "## [1.1.0] - 2026-11-02\n\n### Fixes\n\n- one",
  );
  expect(sectionOf(changelog, "1.1.0")).toBe("### Fixes\n\n- one");
  expect(sectionOf(changelog, "1.0.0")).toBe("First stable release.");
  expect(sectionOf(changelog, "0.9.0")).toBeNull();
});

test.each([
  ["patch", false, "1.2.11"],
  ["minor", false, "1.3.0"],
  ["major", false, "2.0.0"],
  ["minor", true, "1.3.0-rc.3"],
  ["patch", true, "1.2.11-rc.1"],
])("a %s release (candidate: %s) after the latest stable tag is %s", (kind, rc, version) => {
  // Out of order, with candidates and a stray tag: v1.2.10 must not sort before v1.2.9.
  const tags = [
    "v1.2.10",
    "v1.2.9",
    "v1.10.0-rc.1",
    "v1.3.0-rc.2",
    "v1.0.0",
    "v1.3.0-rc.1",
    "vnext",
    "",
  ];
  expect(nextVersion(tags, kind, rc)).toBe(version);
});
