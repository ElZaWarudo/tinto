import { waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoTree } from "../bus/contract";

const listRepoTreeMock = vi.fn<() => Promise<RepoTree>>();

vi.mock("../bus/client", () => ({
  listRepoTree: () => listRepoTreeMock(),
}));

import { repoTreeStore } from "./repoTreeStore";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, reject, resolve };
}

describe("repoTreeStore", () => {
  beforeEach(() => {
    repoTreeStore.reset();
    listRepoTreeMock.mockReset();
  });

  it("queues one trailing refresh instead of losing an invalidation while WSL is loading", async () => {
    const stale = deferred<RepoTree>();
    const fresh = deferred<RepoTree>();
    listRepoTreeMock.mockReturnValueOnce(stale.promise).mockReturnValueOnce(fresh.promise);

    repoTreeStore.refresh("/home/me/repo");
    repoTreeStore.refresh("/home/me/repo");
    repoTreeStore.refresh("/home/me/repo");
    expect(listRepoTreeMock).toHaveBeenCalledTimes(1);

    stale.resolve({ entries: [{ path: "old.txt", is_dir: false }], truncated: false });
    await waitFor(() => expect(listRepoTreeMock).toHaveBeenCalledTimes(2));
    expect(repoTreeStore.get("/home/me/repo").loading).toBe(true);

    fresh.resolve({ entries: [{ path: "new.txt", is_dir: false }], truncated: false });
    await waitFor(() =>
      expect(repoTreeStore.get("/home/me/repo")).toMatchObject({
        loading: false,
        error: false,
        tree: { entries: [{ path: "new.txt", is_dir: false }], truncated: false },
      }),
    );
  });
});
