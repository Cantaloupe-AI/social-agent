import { useEffect, useState } from "react";
import type { Entry } from "@/lib/types";
import { loadTodayEntry } from "@/lib/tauri";

export interface UseTodayEntryResult {
  entry: Entry | null;
  loading: boolean;
  error: string | null;
}

export function useTodayEntry(): UseTodayEntryResult {
  const [entry, setEntry] = useState<Entry | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    loadTodayEntry()
      .then((e) => {
        if (cancelled) return;
        setEntry(e);
        setError(null);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return { entry, loading, error };
}
