import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  Check,
  ExternalLink,
  FileDown,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";

import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useSlides } from "@/hooks/useSlides";
import {
  generateCarouselPdf,
  getCarousel,
  openPdf,
  readSlideScreenshotDataUrl,
} from "@/lib/tauri";
import type { Carousel, Slide, SlideStatus } from "@/lib/types";
import { effectiveSlideTitle } from "@/lib/slide-title";

interface CarouselEditorProps {
  carouselId: string;
  label: string;
  onRename: (nextLabel: string) => Promise<void>;
  onBack: () => void;
  /**
   * Called when this carousel has an active or just-started run. The parent
   * uses this to switch to <GenerationProgressView>. Editor doesn't render
   * progress UI itself.
   */
  onGenerationActive: (carousel: Carousel) => void;
}

const SAVE_DEBOUNCE_MS = 400;

export function CarouselEditor({
  carouselId,
  label,
  onRename,
  onBack,
  onGenerationActive,
}: CarouselEditorProps) {
  const {
    slides,
    loading,
    error,
    create,
    updateContent,
    updateTitle,
    reorder,
    remove,
  } = useSlides(carouselId);
  const [activeId, setActiveId] = useState<string | null>(null);
  // When non-null, that slide's tab is showing an inline title-edit input
  // instead of the title + pencil affordance.
  const [editingTitleId, setEditingTitleId] = useState<string | null>(null);
  // Drag-to-reorder state. We only commit the new order on drop; while
  // dragging this only drives visual indicators.
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    id: string;
    position: "before" | "after";
  } | null>(null);
  // Debug-only: last dragover signal regardless of whether we acted on it.
  // If this updates while `dropTarget` stays null, you know dragover is
  // firing fine and the early-return path (over the source) is the cause.
  const [lastDragoverDebug, setLastDragoverDebug] = useState<{
    targetId: string;
    position: "before" | "after";
    count: number;
  } | null>(null);

  function handleDrop(targetId: string, position: "before" | "after") {
    setDraggingId(null);
    setDropTarget(null);
    if (!draggingId || draggingId === targetId) return;
    const ids = slides.map((s) => s.id);
    const fromIdx = ids.indexOf(draggingId);
    const targetIdx = ids.indexOf(targetId);
    if (fromIdx < 0 || targetIdx < 0) return;
    // Remove from old position first; recompute insert index relative to
    // the post-removal array.
    const without = ids.filter((_, i) => i !== fromIdx);
    let insertIdx = without.indexOf(targetId);
    if (position === "after") insertIdx += 1;
    const next = [
      ...without.slice(0, insertIdx),
      draggingId,
      ...without.slice(insertIdx),
    ];
    if (next.every((id, i) => id === ids[i])) return; // no-op
    void reorder(next);
  }
  const [carouselPdfPath, setCarouselPdfPath] = useState<string | null>(null);
  const [genError, setGenError] = useState<string | null>(null);

  // Keep a valid active tab as slides change.
  useEffect(() => {
    if (slides.length === 0) {
      setActiveId(null);
      return;
    }
    if (!activeId || !slides.some((s) => s.id === activeId)) {
      setActiveId(slides[0].id);
    }
  }, [slides, activeId]);

  // On entry: fetch the carousel. If a run is already active (user reopened
  // the app while the bun driver is still running) hand it straight to the
  // progress view. Otherwise just remember the pdf_path so the Open PDF
  // button is correct.
  useEffect(() => {
    let cancelled = false;
    getCarousel(carouselId)
      .then((c) => {
        if (cancelled || !c) return;
        setCarouselPdfPath(c.pdf_path);
        if (c.status === "generating") onGenerationActive(c);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        console.error("cantalog: getCarousel failed", e);
        setGenError(`Could not load carousel status: ${String(e)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [carouselId, onGenerationActive]);

  async function addSlide() {
    const s = await create();
    setActiveId(s.id);
  }

  async function startGeneration() {
    setGenError(null);
    if (slides.length === 0) {
      setGenError("Add at least one slide before generating.");
      return;
    }
    try {
      await generateCarouselPdf(carouselId);
      // Pull the freshly-updated carousel (Rust just wrote run_dir,
      // run_started_at, status='generating') and hand it to the progress
      // view. If get_carousel fails, surface it loudly — the run is still
      // active in Rust but the UI couldn't switch screens.
      const c = await getCarousel(carouselId);
      if (c) {
        setCarouselPdfPath(c.pdf_path);
        onGenerationActive(c);
      } else {
        setGenError("Generation started, but carousel disappeared from db.");
      }
    } catch (e: unknown) {
      console.error("cantalog: startGeneration failed", e);
      setGenError(String(e));
    }
  }

  const slideCount = slides.length;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeft className="size-3.5" />
          Back
        </Button>
        <div className="flex-1 min-w-0">
          <CarouselTitleInput
            carouselId={carouselId}
            initialLabel={label}
            onRename={onRename}
          />
          <p className="text-xs text-muted-foreground mt-0.5">
            {slideCount} {slideCount === 1 ? "slide" : "slides"}
          </p>
        </div>
        {carouselPdfPath && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              if (carouselPdfPath) void openPdf(carouselPdfPath);
            }}
          >
            <ExternalLink className="size-3.5" />
            Open PDF
          </Button>
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => void startGeneration()}
          disabled={slideCount === 0}
        >
          <FileDown className="size-3.5" />
          Generate PDF
        </Button>
        <Button variant="outline" size="sm" onClick={() => void addSlide()}>
          <Plus className="size-3.5" />
          New slide
        </Button>
      </div>

      {genError && (
        <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
          {genError}
        </div>
      )}

      {error && (
        <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
          {error}
        </div>
      )}

      {loading ? (
        <div className="text-xs text-muted-foreground px-2 py-8">
          Loading slides…
        </div>
      ) : slides.length === 0 ? (
        <div className="text-xs text-muted-foreground px-2 py-8">
          No slides yet — click "New slide" to add the first one.
        </div>
      ) : (
        <Tabs
          orientation="vertical"
          value={activeId ?? undefined}
          onValueChange={setActiveId}
          className="min-h-[320px] relative"
        >
          {/* Live drag-state debug strip. Visible only while a drag is in
              flight; tells us whether the drag started, which slide is
              the drop target, and which half of it. Remove once the
              ordering UX is stable. */}
          {draggingId && (
            <div className="absolute left-40 top-0 z-30 rounded-md bg-amber-100 dark:bg-amber-900/95 text-amber-900 dark:text-amber-100 text-[10px] font-mono px-2 py-1 ring-1 ring-amber-400 shadow-md max-w-[300px] whitespace-nowrap">
              <div>dragging: {draggingId.slice(0, 8)}…</div>
              <div>
                <span className="text-emerald-500">commit:</span>{" "}
                {dropTarget
                  ? `${dropTarget.id.slice(0, 8)}… (${dropTarget.position})`
                  : "—"}
              </div>
              <div>
                <span className="text-blue-400">last hover:</span>{" "}
                {lastDragoverDebug
                  ? `${lastDragoverDebug.targetId.slice(0, 8)}… (${lastDragoverDebug.position}) ×${lastDragoverDebug.count}`
                  : "— (no dragover yet)"}
              </div>
              <div className="text-amber-300/70 mt-0.5">
                If "last hover" shows your own slide id only, you need to
                move the cursor *onto another tab*.
              </div>
            </div>
          )}
          <TabsList>
            {slides.map((s, i) => (
              <SlideTabTrigger
                key={s.id}
                slide={s}
                index={i}
                isEditing={editingTitleId === s.id}
                isDragging={draggingId === s.id}
                dropIndicator={
                  dropTarget?.id === s.id && draggingId !== s.id
                    ? dropTarget.position
                    : null
                }
                onStartEdit={() => {
                  setActiveId(s.id);
                  setEditingTitleId(s.id);
                }}
                onCancelEdit={() => setEditingTitleId(null)}
                onSaveTitle={async (next) => {
                  setEditingTitleId(null);
                  // updateTitle in the hook normalizes empty → null, so
                  // submitting blank reverts to the auto-derived title.
                  await updateTitle(s.id, next);
                }}
                onDragStart={() => {
                  // Cancel any in-flight title edit so the input doesn't
                  // ghost-render under the drag image.
                  setEditingTitleId(null);
                  setDraggingId(s.id);
                  setLastDragoverDebug(null);
                }}
                onDragOverPosition={(position) => {
                  // Always record the raw signal for diagnostics so we can
                  // tell whether dragover is firing AT ALL on this tab,
                  // independent of the source-vs-target gating below.
                  setLastDragoverDebug((prev) => ({
                    targetId: s.id,
                    position,
                    count: prev?.targetId === s.id && prev.position === position
                      ? prev.count + 1
                      : 1,
                  }));
                  if (!draggingId || draggingId === s.id) return;
                  setDropTarget((prev) =>
                    prev?.id === s.id && prev.position === position
                      ? prev
                      : { id: s.id, position },
                  );
                }}
                onDragLeave={() => {
                  setDropTarget((prev) =>
                    prev?.id === s.id ? null : prev,
                  );
                }}
                onDrop={(position) => handleDrop(s.id, position)}
                onDragEnd={() => {
                  setDraggingId(null);
                  setDropTarget(null);
                  setLastDragoverDebug(null);
                }}
              />
            ))}
          </TabsList>

          {slides.map((s) => (
            <TabsContent key={s.id} value={s.id}>
              <SlideEditor
                slideId={s.id}
                initialContent={s.content}
                onSave={(content) => updateContent(s.id, content)}
                onDelete={() => {
                  if (window.confirm("Delete this slide?")) {
                    void remove(s.id);
                  }
                }}
              />
              <SlideRenderPreview slideId={s.id} status={s.status} />
            </TabsContent>
          ))}
        </Tabs>
      )}
    </div>
  );
}

type SaveStatus = "idle" | "saving" | "saved" | "error";

interface CarouselTitleInputProps {
  carouselId: string;
  initialLabel: string;
  onRename: (next: string) => Promise<void>;
}

function CarouselTitleInput({
  carouselId,
  initialLabel,
  onRename,
}: CarouselTitleInputProps) {
  const [value, setValue] = useState(initialLabel);
  const [status, setStatus] = useState<SaveStatus>("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const lastSavedRef = useRef(initialLabel);

  // Reset when we switch to a different carousel.
  useEffect(() => {
    setValue(initialLabel);
    lastSavedRef.current = initialLabel;
    setStatus("idle");
    setErrorMsg(null);
    if (timer.current) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [carouselId]);

  async function save(next: string) {
    const trimmed = next.trim();
    if (trimmed.length === 0) {
      // Don't persist an empty title; surface validation instead.
      setStatus("error");
      setErrorMsg("Title can't be empty.");
      return;
    }
    if (trimmed === lastSavedRef.current) {
      setStatus("idle");
      return;
    }
    setStatus("saving");
    try {
      await onRename(trimmed);
      lastSavedRef.current = trimmed;
      setStatus("saved");
      setErrorMsg(null);
      window.setTimeout(() => {
        setStatus((s) => (s === "saved" ? "idle" : s));
      }, 1000);
    } catch (e: unknown) {
      setStatus("error");
      setErrorMsg(String(e));
    }
  }

  function scheduleSave(next: string) {
    if (timer.current) window.clearTimeout(timer.current);
    setStatus("saving");
    timer.current = window.setTimeout(() => {
      void save(next);
    }, SAVE_DEBOUNCE_MS);
  }

  function flushSave() {
    if (timer.current) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    void save(value);
  }

  return (
    <div className="flex flex-col gap-0.5">
      <input
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          scheduleSave(e.target.value);
        }}
        onBlur={flushSave}
        placeholder="Presentation title"
        aria-label="Presentation title"
        className="w-full bg-transparent text-lg font-semibold tracking-tight outline-none border-b border-transparent hover:border-border focus:border-ring focus-visible:border-ring transition-colors"
      />
      {status === "error" && errorMsg && (
        <span className="text-[11px] text-destructive">{errorMsg}</span>
      )}
    </div>
  );
}

interface SlideEditorProps {
  slideId: string;
  initialContent: string;
  onSave: (content: string) => Promise<void>;
  onDelete: () => void;
}

function SlideEditor({
  slideId,
  initialContent,
  onSave,
  onDelete,
}: SlideEditorProps) {
  const [value, setValue] = useState(initialContent);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">(
    "idle",
  );
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    setValue(initialContent);
    setSaveStatus("idle");
    if (timerRef.current) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [slideId]);

  function scheduleSave(next: string) {
    if (timerRef.current) window.clearTimeout(timerRef.current);
    setSaveStatus("saving");
    timerRef.current = window.setTimeout(async () => {
      try {
        await onSave(next);
        setSaveStatus("saved");
        window.setTimeout(() => {
          setSaveStatus((s) => (s === "saved" ? "idle" : s));
        }, 1000);
      } catch (e: unknown) {
        console.error("cantalog: slide save failed", e);
        setSaveStatus("idle");
      }
    }, SAVE_DEBOUNCE_MS);
  }

  const statusLabel = useMemo(() => {
    if (saveStatus === "saving") return "Saving…";
    if (saveStatus === "saved") return "Saved";
    return " ";
  }, [saveStatus]);

  return (
    <div className="flex flex-col gap-2">
      <div className="grid gap-2 md:grid-cols-2">
        <Textarea
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            scheduleSave(e.target.value);
          }}
          onBlur={() => {
            if (timerRef.current) {
              window.clearTimeout(timerRef.current);
              timerRef.current = null;
            }
            if (value !== initialContent) {
              setSaveStatus("saving");
              void onSave(value)
                .then(() => {
                  setSaveStatus("saved");
                  window.setTimeout(() => {
                    setSaveStatus((s) => (s === "saved" ? "idle" : s));
                  }, 1000);
                })
                .catch((e: unknown) => {
                  console.error("cantalog: slide save failed (blur)", e);
                  setSaveStatus("idle");
                });
            }
          }}
          placeholder="Slide content…"
          className="min-h-[220px] font-mono text-xs"
        />
        <SlideMarkdownPreview source={value} />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">{statusLabel}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={onDelete}
          className="text-destructive hover:text-destructive"
        >
          <Trash2 className="size-3.5" />
          Delete slide
        </Button>
      </div>
    </div>
  );
}

function SlideRenderPreview({
  slideId,
  status,
}: {
  slideId: string;
  status: SlideStatus;
}) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Re-fetch whenever the slide id or status changes — status transitions
  // (idle → generating → reviewing → accepted) signal new versions to render.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    readSlideScreenshotDataUrl(slideId)
      .then((url) => {
        if (cancelled) return;
        setDataUrl(url);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        // Reading a missing or unreadable PNG shouldn't kill the editor,
        // but the user needs to see why no preview is showing.
        console.warn(
          "cantalog: readSlideScreenshotDataUrl failed",
          { slideId },
          e,
        );
        setDataUrl(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slideId, status]);

  if (loading && !dataUrl) {
    return (
      <div className="mt-3 rounded-md border border-input bg-muted/30 px-3 py-6 text-center text-xs text-muted-foreground">
        Loading preview…
      </div>
    );
  }

  if (!dataUrl) {
    return (
      <div className="mt-3 rounded-md border border-dashed border-input bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground">
        No render yet — click Generate PDF to create one.
      </div>
    );
  }

  return (
    <div className="mt-3 rounded-md border border-input bg-muted/30 p-3">
      <div className="text-[11px] uppercase tracking-wider text-muted-foreground mb-2">
        Latest render
      </div>
      {/* Slide is 1080×1350 — display at ~25% so the whole page fits in the editor pane. */}
      <img
        src={dataUrl}
        alt={`Slide ${slideId} render`}
        className="block max-w-full h-auto rounded border border-border/50"
        style={{ maxHeight: 540 }}
      />
    </div>
  );
}

function SlideTabTrigger({
  slide,
  index,
  isEditing,
  isDragging,
  dropIndicator,
  onStartEdit,
  onCancelEdit,
  onSaveTitle,
  onDragStart,
  onDragOverPosition,
  onDragLeave,
  onDrop,
  onDragEnd,
}: {
  slide: Slide;
  index: number;
  isEditing: boolean;
  isDragging: boolean;
  dropIndicator: "before" | "after" | null;
  onStartEdit: () => void;
  onCancelEdit: () => void;
  onSaveTitle: (next: string) => Promise<void>;
  onDragStart: () => void;
  onDragOverPosition: (position: "before" | "after") => void;
  onDragLeave: () => void;
  onDrop: (position: "before" | "after") => void;
  onDragEnd: () => void;
}) {
  const visibleTitle = effectiveSlideTitle(slide.title, slide.content, index);

  if (isEditing) {
    return (
      <SlideTitleEditor
        slideId={slide.id}
        initial={visibleTitle}
        onCancel={onCancelEdit}
        onSubmit={onSaveTitle}
      />
    );
  }

  // Pick insert-before vs insert-after by mouse position over this tab.
  function positionFromEvent(e: React.DragEvent<HTMLElement>): "before" | "after" {
    const rect = e.currentTarget.getBoundingClientRect();
    return e.clientY < rect.top + rect.height / 2 ? "before" : "after";
  }

  return (
    <div className="relative w-full">
      {/* Drop bars: deliberately oversized + lime green so they cannot
          possibly be confused with a styling miss while we debug DnD. */}
      {dropIndicator === "before" && (
        <div
          aria-hidden
          className="pointer-events-none absolute left-0 right-0 -top-1 h-2 rounded-full bg-lime-400 ring-2 ring-lime-200 shadow-lg shadow-lime-500/50 z-20"
        />
      )}
      {dropIndicator === "after" && (
        <div
          aria-hidden
          className="pointer-events-none absolute left-0 right-0 -bottom-1 h-2 rounded-full bg-lime-400 ring-2 ring-lime-200 shadow-lg shadow-lime-500/50 z-20"
        />
      )}
      {/* Translucent half-tab overlay: shows the actual drop ZONE, so the
          user can see exactly which half of the tab counts as "before" vs
          "after". */}
      {dropIndicator === "before" && (
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-1/2 bg-lime-400/20 z-10 rounded-t-md"
        />
      )}
      {dropIndicator === "after" && (
        <div
          aria-hidden
          className="pointer-events-none absolute inset-x-0 bottom-0 h-1/2 bg-lime-400/20 z-10 rounded-b-md"
        />
      )}
      <TabsTrigger
        value={slide.id}
        className={`group/tab w-full justify-between gap-1.5 ${
          isDragging ? "opacity-40 ring-2 ring-amber-400" : ""
        }`}
        title={visibleTitle}
        draggable
        onDragStart={(e) => {
          // dataTransfer needs *something* on Firefox or the drag aborts.
          // We don't actually use it — state is in the parent.
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", slide.id);
          console.log("[dnd] dragstart", { slideId: slide.id });
          onDragStart();
        }}
        onDragOver={(e) => {
          // preventDefault is what tells the browser this is a valid drop
          // target. Without it, drop never fires.
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          const pos = positionFromEvent(e);
          // While diagnosing, log every dragover so we can see exactly
          // which target is being hit. The pill-counter dedupes for the
          // user's eyes; the console gives us the raw stream.
          console.log("[dnd] dragover", {
            targetId: slide.id,
            pos,
            clientY: e.clientY,
          });
          onDragOverPosition(pos);
        }}
        onDragLeave={() => {
          console.log("[dnd] dragleave", { targetId: slide.id });
          onDragLeave();
        }}
        onDrop={(e) => {
          e.preventDefault();
          const pos = positionFromEvent(e);
          console.log("[dnd] drop", { targetId: slide.id, pos });
          onDrop(pos);
        }}
        onDragEnd={() => {
          console.log("[dnd] dragend", { slideId: slide.id });
          onDragEnd();
        }}
      >
        <span className="truncate text-left flex-1 min-w-0">{visibleTitle}</span>
        <span
          role="button"
          aria-label="Edit slide title"
          // The pencil sits inside a <button> (TabsTrigger). Putting a real
          // <button> inside another button is invalid HTML, so we use a
          // role="button" span with the same keyboard semantics. Click is
          // captured before the trigger sees it.
          tabIndex={-1}
          // draggable=false so grabbing the pencil doesn't initiate a drag.
          draggable={false}
          className="shrink-0 rounded p-0.5 opacity-50 group-hover/tab:opacity-100 hover:bg-muted/60 hover:text-foreground transition-opacity"
          onClick={(e) => {
            e.stopPropagation();
            e.preventDefault();
            onStartEdit();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              e.preventDefault();
              onStartEdit();
            }
          }}
        >
          <Pencil className="size-3" />
        </span>
      </TabsTrigger>
    </div>
  );
}

function SlideTitleEditor({
  slideId,
  initial,
  onCancel,
  onSubmit,
}: {
  slideId: string;
  initial: string;
  onCancel: () => void;
  onSubmit: (next: string) => Promise<void>;
}) {
  const [value, setValue] = useState(initial);
  const inputRef = useRef<HTMLInputElement | null>(null);

  // Autofocus + select all on enter — feels natural for "edit this label".
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
    // slideId is part of the dep list so refocusing happens if the user
    // somehow re-enters edit mode for a different slide without unmounting.
  }, [slideId]);

  async function commit() {
    try {
      await onSubmit(value);
    } catch (e: unknown) {
      console.error("cantalog: SlideTitleEditor submit failed", e);
      onCancel();
    }
  }

  return (
    <div className="flex items-center gap-1 px-2 py-1 rounded-md bg-background ring-1 ring-border">
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            void commit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            onCancel();
          }
        }}
        onBlur={() => void commit()}
        placeholder="Slide title"
        className="flex-1 min-w-0 bg-transparent text-sm outline-none"
      />
      <button
        type="button"
        aria-label="Save title"
        // Use onMouseDown to fire before the input's onBlur, so blur sees
        // the post-mouse state and we don't get a double-commit.
        onMouseDown={(e) => {
          e.preventDefault();
          void commit();
        }}
        className="rounded p-0.5 hover:bg-muted/60"
      >
        <Check className="size-3" />
      </button>
      <button
        type="button"
        aria-label="Cancel title edit"
        onMouseDown={(e) => {
          e.preventDefault();
          onCancel();
        }}
        className="rounded p-0.5 hover:bg-muted/60"
      >
        <X className="size-3" />
      </button>
    </div>
  );
}

function SlideMarkdownPreview({ source }: { source: string }) {
  if (source.trim().length === 0) {
    return (
      <div className="min-h-[220px] rounded-md border border-input bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        Preview appears as you type.
      </div>
    );
  }
  return (
    <div className="min-h-[220px] rounded-md border border-input bg-background px-3 py-2 text-sm leading-relaxed slide-md-preview">
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeRaw]}>
        {source}
      </ReactMarkdown>
    </div>
  );
}
