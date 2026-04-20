import { describe, expect, it } from "vitest";
import { mergeEntryWithFetched } from "./merge";
import type { Activity, Entry } from "./types";

function activity(id: string, timestamp: string, included = true): Activity {
  return {
    id,
    source: "git_commit",
    timestamp,
    summary: `commit ${id}`,
    chip: "cantalog",
    included,
  };
}

const ENTRY_STUB: Omit<Entry, "activities"> = {
  date: "2026-04-19",
  thoughts: "shipped",
  created_at: "2026-04-19T15:00:00Z",
  updated_at: "2026-04-19T15:00:00Z",
};

describe("mergeEntryWithFetched", () => {
  it("passes fetched through untouched when no entry exists", () => {
    const fetched = [activity("a", "2026-04-19T10:00:00Z", true)];
    expect(mergeEntryWithFetched(null, fetched)).toEqual(fetched);
  });

  it("marks fetched rows not in the saved entry as excluded", () => {
    const fetched = [
      activity("a", "2026-04-19T10:00:00Z", true),
      activity("b", "2026-04-19T09:00:00Z", true),
    ];
    const entry: Entry = {
      ...ENTRY_STUB,
      activities: [activity("a", "2026-04-19T10:00:00Z", true)],
    };
    const merged = mergeEntryWithFetched(entry, fetched);
    expect(merged.find((x) => x.id === "a")?.included).toBe(true);
    expect(merged.find((x) => x.id === "b")?.included).toBe(false);
  });

  it("keeps saved activities that are no longer in the fetch result", () => {
    const fetched = [activity("b", "2026-04-19T09:00:00Z", true)];
    const entry: Entry = {
      ...ENTRY_STUB,
      activities: [activity("orphan", "2026-04-19T08:00:00Z", true)],
    };
    const merged = mergeEntryWithFetched(entry, fetched);
    expect(merged.map((x) => x.id).sort()).toEqual(["b", "orphan"]);
    expect(merged.find((x) => x.id === "orphan")?.included).toBe(true);
  });

  it("sorts by timestamp descending", () => {
    const fetched = [
      activity("old", "2026-04-19T08:00:00Z", true),
      activity("new", "2026-04-19T15:00:00Z", true),
      activity("mid", "2026-04-19T12:00:00Z", true),
    ];
    const merged = mergeEntryWithFetched(null, fetched);
    expect(merged.map((x) => x.id)).toEqual(["new", "mid", "old"]);
  });
});
