import { useCallback, useEffect, useState } from "react";
import type { Slide } from "@/lib/types";
import {
  createSlide as createSlideCmd,
  deleteSlide as deleteSlideCmd,
  listSlides,
  updateSlideContent as updateSlideContentCmd,
} from "@/lib/tauri";

export interface UseSlidesResult {
  slides: Slide[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: () => Promise<Slide>;
  updateContent: (id: string, content: string) => Promise<void>;
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

  return { slides, loading, error, refresh, create, updateContent, remove };
}
