import { expect, test } from "vitest";

import { sectionFrom, sectionOf, withSection } from "./release.mjs";

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
