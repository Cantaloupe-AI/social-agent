import { useCallback, useEffect, useState } from "react";
import type { Carousel } from "@/lib/types";
import {
  createCarousel as createCarouselCmd,
  deleteCarousel as deleteCarouselCmd,
  listCarousels,
  renameCarousel as renameCarouselCmd,
} from "@/lib/tauri";

export interface UseCarouselsResult {
  carousels: Carousel[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: (label: string) => Promise<Carousel>;
  rename: (id: string, label: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export function useCarousels(): UseCarouselsResult {
  const [carousels, setCarousels] = useState<Carousel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const xs = await listCarousels();
      setCarousels(xs);
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listCarousels()
      .then((xs) => {
        if (cancelled) return;
        setCarousels(xs);
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

  const create = useCallback(async (label: string): Promise<Carousel> => {
    try {
      const c = await createCarouselCmd(label);
      setCarousels((prev) => [c, ...prev]);
      setError(null);
      return c;
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, []);

  const rename = useCallback(async (id: string, label: string) => {
    try {
      await renameCarouselCmd(id, label);
      setCarousels((prev) =>
        prev.map((c) =>
          c.id === id
            ? { ...c, label, updated_at: new Date().toISOString() }
            : c,
        ),
      );
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, []);

  const remove = useCallback(async (id: string) => {
    try {
      await deleteCarouselCmd(id);
      setCarousels((prev) => prev.filter((c) => c.id !== id));
      setError(null);
    } catch (e: unknown) {
      setError(String(e));
      throw e;
    }
  }, []);

  return { carousels, loading, error, refresh, create, rename, remove };
}
