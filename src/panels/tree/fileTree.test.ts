import { describe, it, expect } from "vitest";
import { buildFileTree } from "./fileTree";
import type { RepoStatus, TreeEntry } from "../../bus/contract";

const status = (over: Partial<RepoStatus> = {}): RepoStatus => ({
  modified: [],
  staged: [],
  untracked: [],
  ...over,
});

describe("buildFileTree", () => {
  it("nests files under their folders and marks changed leaves", () => {
    const entries: TreeEntry[] = [
      { path: "src", is_dir: true },
      { path: "src/a.ts", is_dir: false },
      { path: "src/b.ts", is_dir: false },
      { path: "README.md", is_dir: false },
    ];
    const tree = buildFileTree(
      entries,
      status({ modified: ["src/a.ts"], untracked: ["README.md"] }),
    );

    // Dirs first, then files by name → [src/, README.md].
    expect(tree.map((n) => n.name)).toEqual(["src", "README.md"]);
    const src = tree[0];
    expect(src.isDir).toBe(true);
    expect(src.children.map((c) => c.name)).toEqual(["a.ts", "b.ts"]);
    expect(src.children[0].changed).toBe("modified");
    expect(src.children[1].changed).toBeNull();
    expect(tree[1].changed).toBe("untracked");
  });

  it("synthesizes intermediate dirs not present as entries", () => {
    const entries: TreeEntry[] = [{ path: "a/b/c.ts", is_dir: false }];
    const tree = buildFileTree(entries, status());
    expect(tree[0].name).toBe("a");
    expect(tree[0].children[0].name).toBe("b");
    expect(tree[0].children[0].children[0].name).toBe("c.ts");
  });

  it("staged wins over modified for the change mark", () => {
    const entries: TreeEntry[] = [{ path: "x.ts", is_dir: false }];
    const tree = buildFileTree(entries, status({ staged: ["x.ts"], modified: ["x.ts"] }));
    expect(tree[0].changed).toBe("staged");
  });
});
