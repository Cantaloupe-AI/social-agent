import type { Activity, Entry } from "./types";

/**
 * Merges today's freshly-fetched activity list with a previously-saved entry.
 *
 * - If no entry exists, returns fetched activities as-is (all default-included).
 * - Fetched activities whose id is in the saved entry stay included; others get flipped off
 *   so the user sees exactly what they saved last time and can add without losing the
 *   previous shape.
 * - Saved activities whose id is NOT in the fetched list are appended (still included), so
 *   a commit from earlier in the day that's now out of the git log window doesn't vanish.
 * - Result is sorted by timestamp descending so the most recent row is always on top.
 */
export function mergeEntryWithFetched(
  entry: Entry | null,
  fetched: Activity[],
): Activity[] {
  const merged: Activity[] = entry
    ? (() => {
        const savedIds = new Set(entry.activities.map((a) => a.id));
        const fetchedIds = new Set(fetched.map((a) => a.id));
        const out: Activity[] = fetched.map((a) => ({
          ...a,
          included: savedIds.has(a.id),
        }));
        for (const a of entry.activities) {
          if (!fetchedIds.has(a.id)) {
            out.push({ ...a, included: true });
          }
        }
        return out;
      })()
    : [...fetched];

  merged.sort((a, b) =>
    a.timestamp > b.timestamp ? -1 : a.timestamp < b.timestamp ? 1 : 0,
  );
  return merged;
}
