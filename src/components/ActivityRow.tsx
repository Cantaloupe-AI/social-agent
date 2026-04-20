import { GitCommit, Sparkles } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { formatClockTime } from "@/lib/time";
import type { Activity } from "@/lib/types";

export interface ActivityRowProps {
  activity: Activity;
  selected: boolean;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
}

export function ActivityRow({
  activity,
  selected,
  onToggle,
  onSelect,
}: ActivityRowProps) {
  const Icon = activity.source === "git_commit" ? GitCommit : Sparkles;

  return (
    <div
      role="row"
      data-selected={selected}
      onClick={() => onSelect(activity.id)}
      className={cn(
        "group flex items-center gap-3 rounded-md px-2 py-2 text-sm cursor-default",
        "border border-transparent transition-colors",
        selected
          ? "bg-muted/60 border-border"
          : "hover:bg-muted/30",
      )}
    >
      <Checkbox
        checked={activity.included}
        onCheckedChange={() => onToggle(activity.id)}
        onClick={(e) => e.stopPropagation()}
        aria-label={`Include ${activity.summary}`}
      />
      <Icon
        className={cn(
          "size-4 shrink-0",
          activity.source === "git_commit"
            ? "text-emerald-400"
            : "text-amber-400",
        )}
        aria-hidden
      />
      <span className="font-mono text-xs text-muted-foreground tabular-nums w-11 shrink-0">
        {formatClockTime(activity.timestamp)}
      </span>
      <span
        className={cn(
          "flex-1 truncate",
          activity.included ? "text-foreground" : "text-muted-foreground line-through",
        )}
        title={activity.summary}
      >
        {activity.summary}
      </span>
      <span className="font-mono text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground border border-border shrink-0 max-w-[18ch] truncate">
        {activity.chip}
      </span>
    </div>
  );
}
