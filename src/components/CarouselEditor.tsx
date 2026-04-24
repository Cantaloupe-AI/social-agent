import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Plus, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useSlides } from "@/hooks/useSlides";

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
  const { slides, loading, error, create, updateContent, remove } =
    useSlides(carouselId);
  const [activeId, setActiveId] = useState<string | null>(null);

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

  async function addSlide() {
    const s = await create();
    setActiveId(s.id);
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
        <Button variant="outline" size="sm" onClick={() => void addSlide()}>
          <Plus className="size-3.5" />
          New slide
        </Button>
      </div>

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
                Slide {i + 1}
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
        className="min-h-[220px]"
      />
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
