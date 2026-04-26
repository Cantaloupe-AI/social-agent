import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  Check,
  ExternalLink,
  FileDown,
  GripVertical,
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
  /**
   * Persist new per-carousel model overrides for the bun driver's two
   * agents. Pass `null` to revert to the driver's `DEFAULT_MODEL`.
   */
  onUpdateModels: (
    implModel: string | null,
    managerModel: string | null,
  ) => Promise<void>;
  onBack: () => void;
  /**
   * Called when this carousel has an active or just-started run. The parent
   * uses this to switch to <GenerationProgressView>. Editor doesn't render
   * progress UI itself.
   */
  onGenerationActive: (carousel: Carousel) => void;
}

/**
 * Selectable models for the impl + manager agents. Kept in sync with the
 * driver's `DEFAULT_MODEL` fallback in `scripts/generate-carousel.ts` —
 * the DB stores NULL for "use the default" rather than materializing the
 * literal string, so changing the default is a one-line edit there.
 */
const MODEL_OPTIONS: { value: string; label: string }[] = [
  { value: "claude-opus-4-7", label: "Opus 4.7" },
  { value: "claude-sonnet-4-6", label: "Sonnet 4.6" },
];

const DEFAULT_MODEL_VALUE = "claude-opus-4-7";

const SAVE_DEBOUNCE_MS = 400;

export function CarouselEditor({
  carouselId,
  label,
  onRename,
  onUpdateModels,
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
  // Drag-to-reorder state. WKWebView's HTML5 drag-and-drop doesn't
  // reliably fire dragover for in-page drop targets, so we implement
  // drag with pointer events instead — initiated by an explicit grip
  // handle on each tab. We only commit the new order on pointer-up;
  // while dragging this state only drives visual indicators.
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{
    id: string;
    position: "before" | "after";
  } | null>(null);
  const slidesRef = useRef(slides);
  // Keep latest `slides` reachable from the document-level pointer
  // handlers (which capture state at pointerdown time) without forcing
  // the listener-attach effect to re-run on every slide change.
  slidesRef.current = slides;
  const draggingIdRef = useRef<string | null>(null);
  draggingIdRef.current = draggingId;

  function commitDrop(targetId: string, position: "before" | "after") {
    const sourceId = draggingIdRef.current;
    if (!sourceId || sourceId === targetId) return;
    const ids = slidesRef.current.map((s) => s.id);
    const fromIdx = ids.indexOf(sourceId);
    const targetIdx = ids.indexOf(targetId);
    if (fromIdx < 0 || targetIdx < 0) return;
    const without = ids.filter((_, i) => i !== fromIdx);
    let insertIdx = without.indexOf(targetId);
    if (position === "after") insertIdx += 1;
    const next = [
      ...without.slice(0, insertIdx),
      sourceId,
      ...without.slice(insertIdx),
    ];
    if (next.every((id, i) => id === ids[i])) return;
    void reorder(next);
  }

  /**
   * Pointer-events-based drag start, called by the grip-handle on each
   * tab. We deliberately don't use HTML5 drag-and-drop because WKWebView
   * (Tauri's macOS WebView) initiates the drag but doesn't fire dragover
   * on in-page targets, which made the drop indicator never appear.
   */
  function startPointerDrag(slideId: string, e: React.PointerEvent) {
    // Don't preventDefault — that would block subsequent pointermove on
    // some platforms. Just stopPropagation so the click doesn't reach the
    // TabsTrigger.
    e.stopPropagation();
    setEditingTitleId(null);
    setDraggingId(slideId);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "grabbing";

    const findTabUnderPointer = (
      x: number,
      y: number,
    ): { id: string; rect: DOMRect } | null => {
      // Walk up from the topmost element under the pointer until we hit
      // a wrapper with data-slide-id set. Using elementFromPoint instead
      // of dragover lets us route correctly even though WKWebView's
      // HTML5 DnD events are unreliable here.
      const el = document.elementFromPoint(x, y) as HTMLElement | null;
      if (!el) return null;
      const tab = el.closest<HTMLElement>("[data-slide-id]");
      if (!tab) return null;
      const id = tab.getAttribute("data-slide-id");
      if (!id) return null;
      return { id, rect: tab.getBoundingClientRect() };
    };

    const onMove = (ev: PointerEvent) => {
      const hit = findTabUnderPointer(ev.clientX, ev.clientY);
      if (!hit) return;
      if (hit.id === slideId) return; // hovering the source — no commit
      const position: "before" | "after" =
        ev.clientY < hit.rect.top + hit.rect.height / 2 ? "before" : "after";
      setDropTarget((prev) =>
        prev?.id === hit.id && prev.position === position
          ? prev
          : { id: hit.id, position },
      );
    };

    const cleanup = () => {
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
      document.removeEventListener("pointercancel", onCancel);
      document.body.style.userSelect = "";
      document.body.style.cursor = "";
      setDraggingId(null);
      setDropTarget(null);
    };

    const onUp = (ev: PointerEvent) => {
      const hit = findTabUnderPointer(ev.clientX, ev.clientY);
      if (hit && hit.id !== slideId) {
        const position: "before" | "after" =
          ev.clientY < hit.rect.top + hit.rect.height / 2
            ? "before"
            : "after";
        commitDrop(hit.id, position);
      }
      cleanup();
    };

    const onCancel = () => {
      cleanup();
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
    document.addEventListener("pointercancel", onCancel);
  }
  const [carouselPdfPath, setCarouselPdfPath] = useState<string | null>(null);
  const [genError, setGenError] = useState<string | null>(null);
  // Per-carousel model overrides + status. Loaded from the carousel row on
  // mount; updated optimistically when the user changes a dropdown. Status
  // drives the dropdowns' disabled state — during a run the editor is
  // normally swapped out to <GenerationProgressView>, but this guards the
  // brief window before the parent flips view, and a status fetch race.
  const [implModel, setImplModel] = useState<string | null>(null);
  const [managerModel, setManagerModel] = useState<string | null>(null);
  const [carouselStatus, setCarouselStatus] = useState<Carousel["status"]>(
    "idle",
  );

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
        setImplModel(c.impl_model);
        setManagerModel(c.manager_model);
        setCarouselStatus(c.status);
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

  // Optimistically update local state, persist via the parent. On failure
  // the parent hook re-fetches the list (revert), so we re-pull our own
  // copy from getCarousel to stay consistent.
  async function changeImplModel(next: string) {
    const prev = implModel;
    setImplModel(next);
    try {
      await onUpdateModels(next, managerModel);
    } catch (e: unknown) {
      console.error("cantalog: changeImplModel failed", e);
      setImplModel(prev);
      setGenError(`Could not update impl model: ${String(e)}`);
    }
  }

  async function changeManagerModel(next: string) {
    const prev = managerModel;
    setManagerModel(next);
    try {
      await onUpdateModels(implModel, next);
    } catch (e: unknown) {
      console.error("cantalog: changeManagerModel failed", e);
      setManagerModel(prev);
      setGenError(`Could not update manager model: ${String(e)}`);
    }
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
        setCarouselStatus(c.status);
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
        <ModelSelect
          label="Impl"
          value={implModel ?? DEFAULT_MODEL_VALUE}
          onChange={(v) => void changeImplModel(v)}
          disabled={carouselStatus === "generating"}
        />
        <ModelSelect
          label="Manager"
          value={managerModel ?? DEFAULT_MODEL_VALUE}
          onChange={(v) => void changeManagerModel(v)}
          disabled={carouselStatus === "generating"}
        />
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
          className="min-h-[320px]"
        >
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
                  await updateTitle(s.id, next);
                }}
                onGripPointerDown={(e) => startPointerDrag(s.id, e)}
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
  onGripPointerDown,
}: {
  slide: Slide;
  index: number;
  isEditing: boolean;
  isDragging: boolean;
  dropIndicator: "before" | "after" | null;
  onStartEdit: () => void;
  onCancelEdit: () => void;
  onSaveTitle: (next: string) => Promise<void>;
  onGripPointerDown: (e: React.PointerEvent) => void;
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

  return (
    // The data-slide-id attribute is what the parent's pointer-drag
    // walker looks up via document.elementFromPoint().closest(). Without
    // this, drag detection breaks.
    <div className="relative w-full" data-slide-id={slide.id}>
      {/* Drop indicator: a single thin white line at the insert point. */}
      {dropIndicator === "before" && (
        <div
          aria-hidden
          className="pointer-events-none absolute left-1 right-1 -top-px h-px bg-white z-20"
        />
      )}
      {dropIndicator === "after" && (
        <div
          aria-hidden
          className="pointer-events-none absolute left-1 right-1 -bottom-px h-px bg-white z-20"
        />
      )}
      <TabsTrigger
        value={slide.id}
        className={`group/tab w-full justify-between gap-1 ${
          isDragging ? "opacity-40" : ""
        }`}
        title={visibleTitle}
      >
        {/* Grip handle. Drag is a pointer-events-driven custom impl
            because WKWebView's HTML5 dragover doesn't fire reliably on
            in-page targets. The handle has cursor-grab and starts the
            drag in onPointerDown. The rest of the tab area still
            click-selects via TabsTrigger normally. */}
        <span
          role="button"
          aria-label="Reorder slide"
          tabIndex={-1}
          onPointerDown={onGripPointerDown}
          className="shrink-0 -ml-1 cursor-grab active:cursor-grabbing rounded p-0.5 opacity-50 group-hover/tab:opacity-100 hover:bg-muted/60 hover:text-foreground transition-opacity"
        >
          <GripVertical className="size-3.5" />
        </span>
        <span className="truncate text-left flex-1 min-w-0">{visibleTitle}</span>
        <span
          role="button"
          aria-label="Edit slide title"
          tabIndex={-1}
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

/**
 * Compact native `<select>` styled to sit next to the toolbar buttons.
 * Native is intentional: there's no shadcn Select primitive in
 * `src/components/ui/` yet and the dropdowns are A/B testing UI, not a
 * polished surface. Adding a new shadcn dep just for this would violate
 * the project's tight-deps convention (see CLAUDE.md `<dependencies>`).
 */
function ModelSelect({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: string;
  onChange: (next: string) => void;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-center gap-1 text-xs text-muted-foreground">
      <span>{label}</span>
      <select
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
        aria-label={`${label} model`}
        className="h-8 rounded-md border border-input bg-transparent px-2 text-xs outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {MODEL_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </label>
  );
}
