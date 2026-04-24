import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";

import { CarouselEditor } from "@/components/CarouselEditor";
import { Button } from "@/components/ui/button";
import { useCarousels } from "@/hooks/useCarousels";

export function CarouselsView() {
  const { carousels, loading, error, create, rename, remove } = useCarousels();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [composing, setComposing] = useState(false);
  const [draftLabel, setDraftLabel] = useState("");

  if (editingId) {
    const editing = carousels.find((c) => c.id === editingId);
    return (
      <CarouselEditor
        carouselId={editingId}
        label={editing?.label ?? ""}
        onRename={(next) => rename(editingId, next)}
        onBack={() => setEditingId(null)}
      />
    );
  }

  async function submitNew() {
    const label = draftLabel.trim();
    if (!label) {
      setComposing(false);
      setDraftLabel("");
      return;
    }
    try {
      await create(label);
      setDraftLabel("");
      setComposing(false);
    } catch (e: unknown) {
      // Error is also surfaced via the hook's `error` state (banner) so the
      // user sees it; keep the input open so they can fix the label
      // (e.g. unique-constraint violation). Still log so it shows up in the
      // devtools console without depending on the banner being noticed.
      console.error("cantalog: createCarousel failed", e);
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Carousels</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            Author slides. {carousels.length}{" "}
            {carousels.length === 1 ? "carousel" : "carousels"}.
          </p>
        </div>
        {!composing && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setComposing(true)}
          >
            <Plus className="size-3.5" />
            New carousel
          </Button>
        )}
      </div>

      {error && (
        <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
          {error}
        </div>
      )}

      {composing && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submitNew();
          }}
          className="flex items-center gap-2"
        >
          <input
            autoFocus
            value={draftLabel}
            onChange={(e) => setDraftLabel(e.target.value)}
            onBlur={() => {
              if (!draftLabel.trim()) {
                setComposing(false);
              }
            }}
            placeholder="Carousel name"
            className="flex-1 h-8 rounded-md border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
          />
          <Button variant="default" size="sm" type="submit">
            Create
          </Button>
          <Button
            variant="ghost"
            size="sm"
            type="button"
            onClick={() => {
              setComposing(false);
              setDraftLabel("");
            }}
          >
            Cancel
          </Button>
        </form>
      )}

      {loading ? (
        <div className="text-xs text-muted-foreground px-2 py-8">
          Loading carousels…
        </div>
      ) : carousels.length === 0 && !composing ? (
        <div className="text-xs text-muted-foreground px-2 py-8">
          No carousels yet — create one to get started.
        </div>
      ) : (
        <div className="flex flex-col gap-1 rounded-lg border border-border/60 bg-card/40 p-2">
          {carousels.map((c) => (
            <div
              key={c.id}
              className="group flex items-center justify-between gap-2 rounded-md px-2.5 py-2 hover:bg-muted/60"
            >
              <button
                className="flex-1 text-left text-sm font-medium truncate outline-none"
                onClick={() => setEditingId(c.id)}
              >
                {c.label}
              </button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Delete ${c.label}`}
                className="opacity-0 group-hover:opacity-100 transition-opacity"
                onClick={() => {
                  if (
                    window.confirm(
                      `Delete "${c.label}" and all of its slides?`,
                    )
                  ) {
                    void remove(c.id);
                  }
                }}
              >
                <Trash2 className="size-3.5" />
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
