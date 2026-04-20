import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type SaveState = "idle" | "saving" | "saved";

export interface SaveButtonProps {
  state: SaveState;
  onSave: () => void;
  disabled?: boolean;
}

export function SaveButton({ state, onSave, disabled }: SaveButtonProps) {
  const label =
    state === "saving" ? "Saving…" : state === "saved" ? "Saved" : "Save";

  return (
    <div className="flex items-center justify-between gap-3">
      <span className="font-mono text-[11px] text-muted-foreground">
        <kbd className="rounded border border-border px-1 py-0.5 mr-1">⌘</kbd>
        <kbd className="rounded border border-border px-1 py-0.5">↵</kbd> to save
      </span>
      <Button
        onClick={onSave}
        disabled={disabled || state === "saving"}
        className={cn(
          "transition-opacity duration-150",
          state === "saved" && "opacity-80",
        )}
      >
        {label}
      </Button>
    </div>
  );
}
