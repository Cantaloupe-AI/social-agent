import { useCallback, useEffect, useState } from "react";
import type { Slide } from "@/lib/types";
import {
  createSlide as createSlideCmd,
  deleteSlide as deleteSlideCmd,
  listSlides,
  reorderSlides as reorderSlidesCmd,
  updateSlideContent as updateSlideContentCmd,
  updateSlideTitle as updateSlideTitleCmd,
} from "@/lib/tauri";

export interface UseSlidesResult {
  slides: Slide[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: () => Promise<Slide>;
  updateContent: (id: string, content: string) => Promise<void>;
  updateTitle: (id: string, title: string | null) => Promise<void>;
  reorder: (orderedIds: string[]) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export function useSlides(carouselId: string): UseSlidesResult {
  const [slides, setSlides] = useState<Slide[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const xs = await listSlides(carouselId);
      setSlides(xs);
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [carouselId]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listSlides(carouselId)
      .then((xs) => {
        if (cancelled) return;
        setSlides(xs);
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
  }, [carouselId]);

  const create = useCallback(async (): Promise<Slide> => {
    try {
      const s = await createSlideCmd(carouselId);
      setSlides((prev) => [...prev, s]);
      setError(null);
      return s;
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, [carouselId]);

  const updateContent = useCallback(async (id: string, content: string) => {
    // Optimistic update — textarea already shows the new value; persist in the background.
    setSlides((prev) =>
      prev.map((s) =>
        s.id === id
          ? { ...s, content, updated_at: new Date().toISOString() }
          : s,
      ),
    );
    try {
      await updateSlideContentCmd(id, content);
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, []);

  const updateTitle = useCallback(
    async (id: string, title: string | null) => {
      // Optimistic — update local state, persist in the background. Keep the
      // shape consistent: empty string is normalized to null on the Rust
      // side, so we mirror that here so the UI revert-on-blank works
      // without an extra round trip.
      const normalized = title?.trim() ? title.trim() : null;
      setSlides((prev) =>
        prev.map((s) =>
          s.id === id
            ? { ...s, title: normalized, updated_at: new Date().toISOString() }
            : s,
        ),
      );
      try {
        await updateSlideTitleCmd(id, normalized);
        setError(null);
      } catch (e: unknown) {
        console.error("happycampr: updateSlideTitle failed", e);
        setError(String(e));
        throw e;
      }
    },
    [],
  );

  const reorder = useCallback(
    async (orderedIds: string[]) => {
      // Optimistic: rearrange local state immediately, persist after.
      // If the carousel id changes mid-call (user navigates back) the
      // promise still completes; it just updates a stale state, which is
      // harmless because we never expose stale ordering to the user.
      setSlides((prev) => {
        const byId = new Map(prev.map((s) => [s.id, s] as const));
        const next: Slide[] = [];
        orderedIds.forEach((id, idx) => {
          const s = byId.get(id);
          if (s) next.push({ ...s, order_index: idx });
        });
        // Append any slide that wasn't in orderedIds (defensive — caller
        // should always pass every slide).
        for (const s of prev) {
          if (!orderedIds.includes(s.id)) {
            next.push({ ...s, order_index: next.length });
          }
        }
        return next;
      });
      try {
        await reorderSlidesCmd(carouselId, orderedIds);
        setError(null);
      } catch (e: unknown) {
        console.error("happycampr: reorderSlides failed", e);
        setError(String(e));
        // Revert by re-fetching authoritative state.
        try {
          const xs = await listSlides(carouselId);
          setSlides(xs);
        } catch (refreshErr) {
          console.error(
            "happycampr: refetch after failed reorder also failed",
            refreshErr,
          );
        }
        throw e;
      }
    },
    [carouselId],
  );

  const remove = useCallback(async (id: string) => {
    try {
      await deleteSlideCmd(id);
      setSlides((prev) => prev.filter((s) => s.id !== id));
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, []);

  return {
    slides,
    loading,
    error,
    refresh,
    create,
    updateContent,
    updateTitle,
    reorder,
    remove,
  };
}
