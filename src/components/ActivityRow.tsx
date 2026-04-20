import { GitCommit, NotebookPen, Sparkles } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { formatClockTime } from "@/lib/time";
import type { Activity, ActivitySource } from "@/lib/types";

const SOURCE_ICON: Record<ActivitySource, typeof GitCommit> = {
  git_commit: GitCommit,
  claude_code_session: Sparkles,
  note: NotebookPen,
};

const SOURCE_COLOR: Record<ActivitySource, string> = {
  git_commit: "text-emerald-400",
  claude_code_session: "text-amber-400",
  note: "text-sky-400",
};

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
  const Icon = SOURCE_ICON[activity.source];

  return (
    <div
      role="row"
      data-selected={selected}
      onClick={() => onSelect(activity.id)}
      className={cn(
        "group flex items-center gap-4 rounded-md px-3 py-3 text-sm cursor-default",
        "border border-transparent transition-colors",
        "w-full min-w-0",
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
        className={cn("size-4 shrink-0", SOURCE_COLOR[activity.source])}
        aria-hidden
      />
      <span className="font-mono text-xs text-muted-foreground tabular-nums w-11 shrink-0">
        {formatClockTime(activity.timestamp)}
      </span>
      <span
        className={cn(
          "flex-1 min-w-0 truncate",
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
