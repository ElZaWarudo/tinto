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

    // Dirs first, then files by name -> [src/, README.md].
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

  it("normalizes Windows separators before building folders", () => {
    const entries: TreeEntry[] = [
      { path: "docs\\contracts", is_dir: true },
      { path: "docs\\contracts\\bus-contract.md", is_dir: false },
    ];
    const tree = buildFileTree(entries, status({ modified: ["docs/contracts/bus-contract.md"] }));

    expect(tree.map((n) => n.name)).toEqual(["docs"]);
    expect(tree[0].children.map((n) => n.name)).toEqual(["contracts"]);
    expect(tree[0].children[0].children[0].path).toBe("docs/contracts/bus-contract.md");
    expect(tree[0].children[0].children[0].changed).toBe("modified");
  });

  it("staged wins over modified for the change mark", () => {
    const entries: TreeEntry[] = [{ path: "x.ts", is_dir: false }];
    const tree = buildFileTree(entries, status({ staged: ["x.ts"], modified: ["x.ts"] }));
    expect(tree[0].changed).toBe("staged");
  });

  it("propagates hasChanges up to every ancestor folder of a changed file", () => {
    const entries: TreeEntry[] = [
      { path: "a/b/c.ts", is_dir: false },
      { path: "a/d.ts", is_dir: false },
      { path: "clean", is_dir: true },
      { path: "clean/e.ts", is_dir: false },
    ];
    const tree = buildFileTree(entries, status({ modified: ["a/b/c.ts"] }));
    const a = tree.find((n) => n.name === "a")!;
    const b = a.children.find((n) => n.name === "b")!;
    expect(a.hasChanges).toBe(true); // ancestor of the change
    expect(b.hasChanges).toBe(true);
    expect(b.children[0].hasChanges).toBe(true); // the file itself
    expect(a.children.find((n) => n.name === "d.ts")!.hasChanges).toBe(false);
    expect(tree.find((n) => n.name === "clean")!.hasChanges).toBe(false);
  });
});
