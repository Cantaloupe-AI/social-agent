import type { Orientation } from "@/lib/types";

/**
 * Two-button segmented toggle for `vertical` / `landscape`. Used in the
 * new-carousel form (CarouselsView) and in the editor toolbar
 * (CarouselEditor) — both spots already live next to compact native
 * controls (size sm Button, native select), so a custom segmented
 * widget keeps the row visually flat without pulling in a new shadcn
 * primitive (the dependency-tightness argument from CLAUDE.md
 * `<dependencies>`).
 */
export function OrientationToggle({
  value,
  onChange,
  disabled,
}: {
  value: Orientation;
  onChange: (next: Orientation) => void;
  disabled?: boolean;
}) {
  const baseClass =
    "h-8 px-2.5 text-xs border border-input outline-none transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
  return (
    <div
      role="group"
      aria-label="Carousel orientation"
      className="inline-flex"
    >
      <button
        type="button"
        disabled={disabled}
        aria-pressed={value === "vertical"}
        onClick={() => onChange("vertical")}
        className={`${baseClass} rounded-l-md ${
          value === "vertical"
            ? "bg-primary text-primary-foreground"
            : "bg-transparent hover:bg-muted/60"
        }`}
      >
        Vertical
      </button>
      <button
        type="button"
        disabled={disabled}
        aria-pressed={value === "landscape"}
        onClick={() => onChange("landscape")}
        className={`${baseClass} -ml-px rounded-r-md ${
          value === "landscape"
            ? "bg-primary text-primary-foreground"
            : "bg-transparent hover:bg-muted/60"
        }`}
      >
        Landscape
      </button>
    </div>
  );
}
