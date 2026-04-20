import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

import { useTodayEntry } from "./useTodayEntry";
import * as tauri from "@/lib/tauri";
import type { Entry } from "@/lib/types";

vi.mock("@/lib/tauri", () => ({
  fetchTodayActivities: vi.fn(),
  loadTodayEntry: vi.fn(),
  saveEntry: vi.fn(),
  loadConfig: vi.fn(),
  saveConfig: vi.fn(),
}));

const mockLoad = tauri.loadTodayEntry as ReturnType<typeof vi.fn>;

describe("useTodayEntry", () => {
  beforeEach(() => {
    mockLoad.mockReset();
  });

  it("returns null when no entry exists for today", async () => {
    mockLoad.mockResolvedValueOnce(null);
    const { result } = renderHook(() => useTodayEntry());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entry).toBeNull();
    expect(result.current.error).toBeNull();
  });

  it("returns the stored entry when one exists", async () => {
    const stored: Entry = {
      date: "2026-04-19",
      activities: [],
      thoughts: "Shipped the backend today.",
      created_at: "2026-04-19T15:00:00Z",
      updated_at: "2026-04-19T15:00:00Z",
    };
    mockLoad.mockResolvedValueOnce(stored);
    const { result } = renderHook(() => useTodayEntry());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entry).toEqual(stored);
  });

  it("captures load errors without crashing", async () => {
    mockLoad.mockRejectedValueOnce("db broke");
    const { result } = renderHook(() => useTodayEntry());
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entry).toBeNull();
    expect(result.current.error).toBe("db broke");
  });
});
