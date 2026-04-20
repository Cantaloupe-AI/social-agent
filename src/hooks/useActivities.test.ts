import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import { useActivities } from "./useActivities";
import * as tauri from "@/lib/tauri";
import type { Activity } from "@/lib/types";

vi.mock("@/lib/tauri", () => ({
  fetchTodayActivities: vi.fn(),
  loadTodayEntry: vi.fn(),
  saveEntry: vi.fn(),
  loadConfig: vi.fn(),
  saveConfig: vi.fn(),
}));

const mockFetch = tauri.fetchTodayActivities as ReturnType<typeof vi.fn>;

function fixture(id: string, included = true): Activity {
  return {
    id,
    source: "git_commit",
    timestamp: "2026-04-19T14:32:11Z",
    summary: `commit ${id}`,
    chip: "cantalog",
    included,
  };
}

describe("useActivities", () => {
  beforeEach(() => {
    mockFetch.mockReset();
  });

  it("fetches activities on mount and exposes them", async () => {
    mockFetch.mockResolvedValueOnce([fixture("a"), fixture("b")]);
    const { result } = renderHook(() => useActivities());

    expect(result.current.loading).toBe(true);

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBeNull();
    expect(result.current.activities.map((a) => a.id)).toEqual(["a", "b"]);
  });

  it("toggle() flips the included flag on the matching activity only", async () => {
    mockFetch.mockResolvedValueOnce([
      fixture("a", true),
      fixture("b", true),
    ]);
    const { result } = renderHook(() => useActivities());
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      result.current.toggle("a");
    });

    expect(result.current.activities.find((a) => a.id === "a")?.included).toBe(
      false,
    );
    expect(result.current.activities.find((a) => a.id === "b")?.included).toBe(
      true,
    );
  });

  it("exposes the rejection as an error string", async () => {
    mockFetch.mockRejectedValueOnce("boom");
    const { result } = renderHook(() => useActivities());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBe("boom");
    expect(result.current.activities).toEqual([]);
  });
});
