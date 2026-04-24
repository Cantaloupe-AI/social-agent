import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, FileDown, Loader2, Plus, Trash2, X } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";

import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useSlides } from "@/hooks/useSlides";
import {
  cancelGeneration,
  generateCarouselPdf,
  getCarousel,
} from "@/lib/tauri";
import type { CarouselStatus, SlideStatus } from "@/lib/types";

interface CarouselEditorProps {
  carouselId: string;
  label: string;
  onRename: (nextLabel: string) => Promise<void>;
  onBack: () => void;
}

const SAVE_DEBOUNCE_MS = 400;

export function CarouselEditor({
  carouselId,
  label,
  onRename,
  onBack,
}: CarouselEditorProps) {
  const { slides, loading, error, create, updateContent, remove, refresh } =
    useSlides(carouselId);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [carouselStatus, setCarouselStatus] = useState<CarouselStatus>("idle");
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

  // Initial fetch of the carousel's stored status (in case a previous run
  // left it 'done' or 'failed' — we want to reflect that on entry).
  useEffect(() => {
    let cancelled = false;
    getCarousel(carouselId)
      .then((c) => {
        if (!cancelled && c) setCarouselStatus(c.status);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [carouselId]);

  // Poll while generating. Stops when status leaves 'generating'.
  useEffect(() => {
    if (carouselStatus !== "generating") return;
    let cancelled = false;
    const tick = async () => {
      try {
        const c = await getCarousel(carouselId);
        if (cancelled) return;
        if (c) setCarouselStatus(c.status);
        await refresh();
      } catch {
        // Best-effort polling — surface errors via genError on user actions only.
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 2000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [carouselStatus, carouselId, refresh]);

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
      setCarouselStatus("generating");
    } catch (e: unknown) {
      setGenError(String(e));
    }
  }

  async function stopGeneration() {
    try {
      await cancelGeneration(carouselId);
      setCarouselStatus("failed");
    } catch (e: unknown) {
      setGenError(String(e));
    }
  }

  const slideCount = slides.length;
  const isGenerating = carouselStatus === "generating";

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
          <p className="text-xs text-muted-foreground mt-0.5 flex items-center gap-2">
            <span>
              {slideCount} {slideCount === 1 ? "slide" : "slides"}
            </span>
            <CarouselStatusPill status={carouselStatus} />
          </p>
        </div>
        {isGenerating ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => void stopGeneration()}
          >
            <X className="size-3.5" />
            Cancel
          </Button>
        ) : (
          <Button
            variant="outline"
            size="sm"
            onClick={() => void startGeneration()}
            disabled={slideCount === 0}
          >
            <FileDown className="size-3.5" />
            Generate PDF
          </Button>
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => void addSlide()}
          disabled={isGenerating}
        >
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
              <TabsTrigger key={s.id} value={s.id}>
                <span>Slide {i + 1}</span>
                <SlideStatusPill status={s.status} />
              </TabsTrigger>
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
      } catch {
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
                .catch(() => setSaveStatus("idle"));
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

function CarouselStatusPill({ status }: { status: CarouselStatus }) {
  if (status === "idle") return null;
  if (status === "generating") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-amber-100 dark:bg-amber-900/40 px-1.5 py-0.5 text-[10px] font-medium text-amber-900 dark:text-amber-200">
        <Loader2 className="size-2.5 animate-spin" />
        generating
      </span>
    );
  }
  if (status === "done") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-emerald-100 dark:bg-emerald-900/40 px-1.5 py-0.5 text-[10px] font-medium text-emerald-900 dark:text-emerald-200">
        done
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-destructive/10 px-1.5 py-0.5 text-[10px] font-medium text-destructive">
      failed
    </span>
  );
}

function SlideStatusPill({ status }: { status: SlideStatus }) {
  if (status === "idle") return null;
  const palette: Record<Exclude<SlideStatus, "idle">, string> = {
    queued:
      "bg-muted text-muted-foreground",
    generating:
      "bg-amber-100 dark:bg-amber-900/40 text-amber-900 dark:text-amber-200",
    reviewing:
      "bg-blue-100 dark:bg-blue-900/40 text-blue-900 dark:text-blue-200",
    accepted:
      "bg-emerald-100 dark:bg-emerald-900/40 text-emerald-900 dark:text-emerald-200",
    failed:
      "bg-destructive/10 text-destructive",
  };
  const showSpinner = status === "generating" || status === "reviewing";
  return (
    <span
      className={`ml-1.5 inline-flex items-center gap-0.5 rounded-full px-1 py-0.5 text-[9px] font-medium ${palette[status]}`}
    >
      {showSpinner && <Loader2 className="size-2 animate-spin" />}
      {status}
    </span>
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
