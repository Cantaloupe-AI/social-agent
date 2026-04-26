import { useCallback, useEffect, useState } from "react";
import type { Carousel } from "@/lib/types";
import {
  createCarousel as createCarouselCmd,
  deleteCarousel as deleteCarouselCmd,
  listCarousels,
  renameCarousel as renameCarouselCmd,
  updateCarouselModels as updateCarouselModelsCmd,
} from "@/lib/tauri";

export interface UseCarouselsResult {
  carousels: Carousel[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  create: (label: string) => Promise<Carousel>;
  rename: (id: string, label: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  updateModels: (
    id: string,
    implModel: string | null,
    managerModel: string | null,
  ) => Promise<void>;
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

  const updateModels = useCallback(
    async (
      id: string,
      implModel: string | null,
      managerModel: string | null,
    ) => {
      // Mirror the Rust normalization: empty / whitespace strings reset to
      // null (auto-default). Keeps the optimistic local state in sync with
      // what `update_carousel_models` will actually persist.
      const implNorm = implModel?.trim() ? implModel.trim() : null;
      const managerNorm = managerModel?.trim() ? managerModel.trim() : null;
      setCarousels((prev) =>
        prev.map((c) =>
          c.id === id
            ? {
                ...c,
                impl_model: implNorm,
                manager_model: managerNorm,
                updated_at: new Date().toISOString(),
              }
            : c,
        ),
      );
      try {
        await updateCarouselModelsCmd(id, implNorm, managerNorm);
        setError(null);
      } catch (e: unknown) {
        console.error("cantalog: updateCarouselModels failed", e);
        setError(String(e));
        // Revert by re-fetching authoritative state.
        try {
          const xs = await listCarousels();
          setCarousels(xs);
        } catch (refreshErr) {
          console.error(
            "cantalog: refetch after failed updateModels also failed",
            refreshErr,
          );
        }
        throw e;
      }
    },
    [],
  );

  return {
    carousels,
    loading,
    error,
    refresh,
    create,
    rename,
    remove,
    updateModels,
  };
}
