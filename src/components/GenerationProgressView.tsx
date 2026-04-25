import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  Loader2,
  X,
  XCircle,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  cancelGeneration,
  getCarousel,
  listSlideFeedback,
  listSlideVersions,
  listSlides,
  openPdf,
  readRunLogTail,
  readSlideScreenshotDataUrl,
} from "@/lib/tauri";
import { effectiveSlideTitle } from "@/lib/slide-title";
import type {
  Carousel,
  CarouselStatus,
  Slide,
  SlideFeedback,
  SlideStatus,
  SlideVersion,
} from "@/lib/types";

const SLIDE_POLL_MS = 2000;
const LOG_POLL_MS = 1500;
const LOG_TAIL_LINES = 200;

interface GenerationProgressViewProps {
  carousel: Carousel;
  onClose: () => void;
}

export function GenerationProgressView({
  carousel: initialCarousel,
  onClose,
}: GenerationProgressViewProps) {
  const [carousel, setCarousel] = useState<Carousel>(initialCarousel);
  const [slides, setSlides] = useState<Slide[]>([]);
  const [selectedSlideId, setSelectedSlideId] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);

  const isActive = carousel.status === "generating";

  // Poll carousel + slide list while generating, plus a final fetch once
  // we transition to a terminal state so we land on the most recent data.
  useEffect(() => {
    let cancelled = false;
    const fetchAll = async () => {
      try {
        const [c, ss] = await Promise.all([
          getCarousel(carousel.id),
          listSlides(carousel.id),
        ]);
        if (cancelled) return;
        if (c) setCarousel(c);
        setSlides(ss);
        setPollError(null);
      } catch (e: unknown) {
        if (cancelled) return;
        console.warn("cantalog: progress poll failed", e);
        setPollError(String(e));
      }
    };
    void fetchAll();
    if (!isActive) return;
    const id = window.setInterval(() => void fetchAll(), SLIDE_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [carousel.id, isActive]);

  // Default selection: first slide that's still active, else first slide.
  useEffect(() => {
    if (slides.length === 0) {
      setSelectedSlideId(null);
      return;
    }
    if (selectedSlideId && slides.some((s) => s.id === selectedSlideId)) return;
    const firstActive = slides.find(
      (s) =>
        s.status === "generating" ||
        s.status === "reviewing" ||
        s.status === "queued",
    );
    setSelectedSlideId((firstActive ?? slides[0]).id);
  }, [slides, selectedSlideId]);

  const selectedSlide = useMemo(
    () => slides.find((s) => s.id === selectedSlideId) ?? null,
    [slides, selectedSlideId],
  );

  const elapsed = useElapsed(carousel.run_started_at, carousel.run_finished_at);

  async function handleCancel() {
    try {
      await cancelGeneration(carousel.id);
    } catch (e: unknown) {
      console.error("cantalog: cancelGeneration failed", e);
      setPollError(`Cancel failed: ${String(e)}`);
    }
  }

  return (
    <div className="flex flex-col gap-3 h-full min-h-[600px]">
      {/* Header */}
      <div className="flex items-center gap-2 pb-2 border-b border-border/60">
        <Button variant="ghost" size="sm" onClick={onClose}>
          <ArrowLeft className="size-3.5" />
          Back
        </Button>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-sm font-semibold tracking-tight truncate">
              {carousel.label}
            </h1>
            <CarouselStatusBadge status={carousel.status} />
          </div>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            {elapsed ? `Elapsed ${elapsed}` : "Not started"}
            {carousel.status === "done" && carousel.pdf_path
              ? " · PDF ready"
              : ""}
          </p>
        </div>
        {carousel.status === "done" && carousel.pdf_path && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              if (carousel.pdf_path) void openPdf(carousel.pdf_path);
            }}
          >
            <ExternalLink className="size-3.5" />
            Open PDF
          </Button>
        )}
        {isActive && (
          <Button variant="outline" size="sm" onClick={() => void handleCancel()}>
            <X className="size-3.5" />
            Cancel
          </Button>
        )}
      </div>

      {pollError && (
        <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
          {pollError}
        </div>
      )}

      {/* Two-column body: slide list + detail */}
      <div className="grid grid-cols-[220px_1fr] gap-3 flex-1 min-h-0">
        <SlideListPanel
          slides={slides}
          selectedId={selectedSlideId}
          onSelect={setSelectedSlideId}
        />
        <SlideDetailPanel slide={selectedSlide} />
      </div>

      {/* Run-log panel */}
      <RunLogPanel carouselId={carousel.id} active={isActive} />
    </div>
  );
}

// ─── Slide list ────────────────────────────────────────────────────────────

function SlideListPanel({
  slides,
  selectedId,
  onSelect,
}: {
  slides: Slide[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  if (slides.length === 0) {
    return (
      <div className="rounded-md border border-input bg-muted/20 px-3 py-4 text-xs text-muted-foreground">
        No slides.
      </div>
    );
  }
  return (
    <div className="rounded-md border border-input bg-muted/10 p-1 overflow-y-auto">
      {slides.map((s, i) => {
        const isSelected = s.id === selectedId;
        const title = effectiveSlideTitle(s.title, s.content, i);
        return (
          <button
            key={s.id}
            onClick={() => onSelect(s.id)}
            title={title}
            className={`w-full text-left px-2.5 py-2 rounded-md flex items-center gap-2 ${
              isSelected
                ? "bg-accent text-accent-foreground"
                : "hover:bg-muted/60"
            }`}
          >
            <SlideStatusIcon status={s.status} />
            <span className="text-sm font-medium truncate min-w-0 flex-1">
              {title}
            </span>
            {s.last_error && !isSelected && (
              <span
                className="ml-auto text-[10px] text-destructive truncate max-w-[80px]"
                title={s.last_error}
              >
                error
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

// ─── Slide detail (latest screenshot + version history) ───────────────────

function SlideDetailPanel({ slide }: { slide: Slide | null }) {
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [versions, setVersions] = useState<SlideVersion[]>([]);
  const [feedbackByVersion, setFeedbackByVersion] = useState<
    Record<string, SlideFeedback[]>
  >({});
  const [loadError, setLoadError] = useState<string | null>(null);

  const slideId = slide?.id ?? null;
  const status = slide?.status ?? "idle";

  useEffect(() => {
    if (!slideId) {
      setScreenshotUrl(null);
      setVersions([]);
      setFeedbackByVersion({});
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const [url, vs] = await Promise.all([
          readSlideScreenshotDataUrl(slideId),
          listSlideVersions(slideId),
        ]);
        if (cancelled) return;
        setScreenshotUrl(url);
        setVersions(vs);

        // Fetch feedback for every version in parallel.
        const fbEntries = await Promise.all(
          vs.map(async (v) => {
            try {
              const fb = await listSlideFeedback(v.id);
              return [v.id, fb] as const;
            } catch (e) {
              console.warn(
                "cantalog: listSlideFeedback failed for",
                v.id,
                e,
              );
              return [v.id, []] as const;
            }
          }),
        );
        if (cancelled) return;
        setFeedbackByVersion(Object.fromEntries(fbEntries));
        setLoadError(null);
      } catch (e: unknown) {
        if (cancelled) return;
        console.warn("cantalog: slide detail load failed", e);
        setLoadError(String(e));
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
    // status is included so we re-fetch on every transition (new version etc.)
  }, [slideId, status]);

  if (!slide) {
    return (
      <div className="rounded-md border border-input bg-muted/20 px-3 py-4 text-xs text-muted-foreground">
        Select a slide.
      </div>
    );
  }

  return (
    <div className="rounded-md border border-input bg-card/40 p-3 overflow-y-auto">
      <div className="flex items-center gap-2 mb-2 min-w-0">
        <span className="text-[11px] uppercase tracking-wider text-muted-foreground shrink-0">
          Slide {slide.order_index + 1}
        </span>
        <h2
          className="text-sm font-semibold truncate min-w-0"
          title={effectiveSlideTitle(slide.title, slide.content, slide.order_index)}
        >
          {effectiveSlideTitle(slide.title, slide.content, slide.order_index)}
        </h2>
        <SlideStatusBadge status={slide.status} />
        {slide.last_error && (
          <span
            className="text-[11px] text-destructive truncate max-w-[300px]"
            title={slide.last_error}
          >
            {slide.last_error}
          </span>
        )}
      </div>

      {loadError && (
        <div className="mb-2 text-[11px] text-destructive">{loadError}</div>
      )}

      {/* Latest screenshot */}
      {screenshotUrl ? (
        <img
          src={screenshotUrl}
          alt={`Slide ${slide.order_index + 1} latest render`}
          className="block w-full h-auto rounded border border-border/50 bg-background"
          style={{ maxHeight: 540 }}
        />
      ) : (
        <div className="rounded border border-dashed border-input bg-muted/20 px-3 py-8 text-center text-xs text-muted-foreground">
          {status === "generating" || status === "reviewing"
            ? "Render in progress…"
            : "No render yet."}
        </div>
      )}

      {/* Version history */}
      <div className="mt-3">
        <h3 className="text-[11px] uppercase tracking-wider text-muted-foreground mb-1.5">
          Versions ({versions.length})
        </h3>
        {versions.length === 0 ? (
          <p className="text-xs text-muted-foreground">No versions yet.</p>
        ) : (
          <ol className="flex flex-col gap-1.5">
            {versions.map((v, idx) => (
              <SlideVersionRow
                key={v.id}
                version={v}
                feedback={feedbackByVersion[v.id] ?? []}
                defaultOpen={idx === 0}
              />
            ))}
          </ol>
        )}
      </div>
    </div>
  );
}

function SlideVersionRow({
  version,
  feedback,
  defaultOpen,
}: {
  version: SlideVersion;
  feedback: SlideFeedback[];
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const latest = feedback[feedback.length - 1] ?? null;
  return (
    <li className="rounded border border-border/60 bg-background">
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 text-left hover:bg-muted/40"
      >
        {open ? (
          <ChevronDown className="size-3.5" />
        ) : (
          <ChevronRight className="size-3.5" />
        )}
        <span className="text-xs font-mono">v{version.version_number}</span>
        <FeedbackVerdict feedback={latest} />
        <span className="ml-auto text-[10px] text-muted-foreground">
          {new Date(version.created_at).toLocaleTimeString()}
        </span>
      </button>
      {open && (
        <div className="px-2.5 pb-2.5 pt-1 text-xs space-y-1.5">
          <div className="flex flex-wrap gap-2 text-[10px] text-muted-foreground">
            {version.screenshot_path && <span>screenshot ✓</span>}
            {version.pdf_path && <span>pdf ✓</span>}
          </div>
          {latest && !latest.accepted && latest.feedback ? (
            <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed text-foreground/90 border-l-2 border-destructive/40 pl-2">
              {latest.feedback}
            </pre>
          ) : latest?.accepted ? (
            <p className="text-emerald-600 dark:text-emerald-400">
              Accepted by manager.
            </p>
          ) : (
            <p className="text-muted-foreground">No review yet.</p>
          )}
        </div>
      )}
    </li>
  );
}

function FeedbackVerdict({ feedback }: { feedback: SlideFeedback | null }) {
  if (!feedback) {
    return (
      <span className="text-[10px] text-muted-foreground">no review yet</span>
    );
  }
  if (feedback.accepted) {
    return (
      <span className="inline-flex items-center gap-0.5 text-[10px] text-emerald-700 dark:text-emerald-300">
        <CheckCircle2 className="size-3" />
        accepted
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-0.5 text-[10px] text-destructive">
      <XCircle className="size-3" />
      rejected
    </span>
  );
}

// ─── Run log ──────────────────────────────────────────────────────────────

function RunLogPanel({
  carouselId,
  active,
}: {
  carouselId: string;
  active: boolean;
}) {
  const [tail, setTail] = useState<string>("");
  const [logError, setLogError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const userScrolledUp = useRef(false);

  // Poll the log tail. After the run finishes, do one final fetch and stop.
  useEffect(() => {
    let cancelled = false;
    const fetchTail = async () => {
      try {
        const t = await readRunLogTail(carouselId, LOG_TAIL_LINES);
        if (cancelled) return;
        setTail(t);
        setLogError(null);
      } catch (e: unknown) {
        if (cancelled) return;
        console.warn("cantalog: readRunLogTail failed", e);
        setLogError(String(e));
      }
    };
    void fetchTail();
    if (!active) return;
    const id = window.setInterval(() => void fetchTail(), LOG_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [carouselId, active]);

  // Auto-scroll to bottom when new content arrives, unless user has scrolled up.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    if (userScrolledUp.current) return;
    el.scrollTop = el.scrollHeight;
  }, [tail]);

  function handleScroll() {
    const el = scrollRef.current;
    if (!el) return;
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    userScrolledUp.current = distFromBottom > 24;
  }

  return (
    <div className="rounded-md border border-input bg-muted/20">
      <div className="flex items-center gap-2 px-2.5 py-1.5 border-b border-border/40">
        <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
          run.log
        </span>
        {active && (
          <Loader2 className="size-3 animate-spin text-muted-foreground" />
        )}
        <span className="ml-auto text-[10px] text-muted-foreground">
          last {LOG_TAIL_LINES} lines · auto-scrolls
        </span>
      </div>
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="px-2.5 py-1.5 text-[11px] leading-relaxed font-mono whitespace-pre-wrap overflow-y-auto bg-background/40"
        style={{ height: 200 }}
      >
        {logError ? (
          <span className="text-destructive">{logError}</span>
        ) : tail.length === 0 ? (
          <span className="text-muted-foreground">
            Waiting for the bun driver to start writing…
          </span>
        ) : (
          tail
        )}
      </div>
    </div>
  );
}

// ─── Status indicators ────────────────────────────────────────────────────

function CarouselStatusBadge({ status }: { status: CarouselStatus }) {
  if (status === "idle") return null;
  const map: Record<Exclude<CarouselStatus, "idle">, [string, string]> = {
    generating: [
      "bg-amber-100 dark:bg-amber-900/40 text-amber-900 dark:text-amber-200",
      "generating",
    ],
    done: [
      "bg-emerald-100 dark:bg-emerald-900/40 text-emerald-900 dark:text-emerald-200",
      "done",
    ],
    failed: ["bg-destructive/10 text-destructive", "failed"],
  };
  const [cls, label] = map[status];
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${cls}`}
    >
      {status === "generating" && <Loader2 className="size-2.5 animate-spin" />}
      {label}
    </span>
  );
}

function SlideStatusBadge({ status }: { status: SlideStatus }) {
  if (status === "idle") return null;
  const palette: Record<Exclude<SlideStatus, "idle">, string> = {
    queued: "bg-muted text-muted-foreground",
    generating:
      "bg-amber-100 dark:bg-amber-900/40 text-amber-900 dark:text-amber-200",
    reviewing:
      "bg-blue-100 dark:bg-blue-900/40 text-blue-900 dark:text-blue-200",
    accepted:
      "bg-emerald-100 dark:bg-emerald-900/40 text-emerald-900 dark:text-emerald-200",
    failed: "bg-destructive/10 text-destructive",
  };
  const showSpinner = status === "generating" || status === "reviewing";
  return (
    <span
      className={`inline-flex items-center gap-0.5 rounded-full px-1.5 py-0.5 text-[10px] font-medium ${palette[status]}`}
    >
      {showSpinner && <Loader2 className="size-2.5 animate-spin" />}
      {status}
    </span>
  );
}

function SlideStatusIcon({ status }: { status: SlideStatus }) {
  const cls = "size-3.5 shrink-0";
  switch (status) {
    case "accepted":
      return (
        <CheckCircle2
          className={`${cls} text-emerald-600 dark:text-emerald-400`}
        />
      );
    case "failed":
      return <XCircle className={`${cls} text-destructive`} />;
    case "generating":
    case "reviewing":
      return <Loader2 className={`${cls} animate-spin text-amber-600`} />;
    case "queued":
      return <span className={`${cls} text-muted-foreground`}>○</span>;
    case "idle":
    default:
      return <span className={`${cls} text-muted-foreground`}>·</span>;
  }
}

// ─── Elapsed time hook ────────────────────────────────────────────────────

function useElapsed(
  startIso: string | null,
  endIso: string | null,
): string | null {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (endIso) {
      // Run is finished; pin "now" to the end so the timer stops counting.
      setNow(new Date(endIso).getTime());
      return;
    }
    if (!startIso) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [startIso, endIso]);
  return useMemo(() => {
    if (!startIso) return null;
    const start = new Date(startIso).getTime();
    const end = endIso ? new Date(endIso).getTime() : now;
    const seconds = Math.max(0, Math.round((end - start) / 1000));
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }, [startIso, endIso, now]);
}

