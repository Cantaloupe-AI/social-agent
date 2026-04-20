import { useEffect, useState } from "react";
import type { Activity } from "@/lib/types";
import { fetchTodayActivities } from "@/lib/tauri";

export interface UseActivitiesResult {
  activities: Activity[];
  setActivities: (next: Activity[]) => void;
  toggle: (id: string) => void;
  loading: boolean;
  error: string | null;
}

export function useActivities(): UseActivitiesResult {
  const [activities, setActivities] = useState<Activity[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetchTodayActivities()
      .then((xs) => {
        if (cancelled) return;
        setActivities(xs);
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

  function toggle(id: string): void {
    setActivities((prev) =>
      prev.map((a) => (a.id === id ? { ...a, included: !a.included } : a)),
    );
  }

  return { activities, setActivities, toggle, loading, error };
}
