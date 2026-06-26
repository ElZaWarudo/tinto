import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  forgetRecentWorkbench,
  getRecentWorkbenches,
  markRecentWorkbench,
  sortByRecency,
} from "./recentWorkbenches";

describe("recentWorkbenches", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("starts empty", () => {
    expect(getRecentWorkbenches()).toEqual([]);
  });

  it("pushes new names to the front, de-duplicating", () => {
    markRecentWorkbench("Work");
    markRecentWorkbench("Side");
    markRecentWorkbench("Client X");
    expect(getRecentWorkbenches()).toEqual(["Client X", "Side", "Work"]);

    markRecentWorkbench("Work"); // re-activate → moves to front, no duplicate
    expect(getRecentWorkbenches()).toEqual(["Work", "Client X", "Side"]);
  });

  it("ignores empty or whitespace-only names", () => {
    markRecentWorkbench("");
    markRecentWorkbench("   ");
    expect(getRecentWorkbenches()).toEqual([]);
  });

  it("forgets a name (used on delete and before rename)", () => {
    markRecentWorkbench("Work");
    markRecentWorkbench("Side");
    forgetRecentWorkbench("Work");
    expect(getRecentWorkbenches()).toEqual(["Side"]);
  });

  it("sorts a set of names by recency and appends unknown names at the end", () => {
    markRecentWorkbench("Work");
    markRecentWorkbench("Side");
    markRecentWorkbench("Client X");

    const sorted = sortByRecency(["Unknown A", "Work", "Unknown B", "Side", "Client X"]);
    expect(sorted).toEqual(["Client X", "Side", "Work", "Unknown A", "Unknown B"]);
  });

  it("is robust against malformed localStorage payloads", () => {
    localStorage.setItem("tinto:recent-workbenches:v1", "{not json");
    expect(getRecentWorkbenches()).toEqual([]);

    localStorage.setItem("tinto:recent-workbenches:v1", JSON.stringify(["Work", 42, null]));
    expect(getRecentWorkbenches()).toEqual(["Work"]);
  });
});
