import { ActivityRow } from "./ActivityRow";
import type { Activity } from "@/lib/types";

export interface ActivityListProps {
  activities: Activity[];
  selectedId: string | null;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
  loading: boolean;
}

export function ActivityList({
  activities,
  selectedId,
  onToggle,
  onSelect,
  loading,
}: ActivityListProps) {
  if (loading) {
    return (
      <div className="text-xs text-muted-foreground px-2 py-8">
        Loading today's activity…
      </div>
    );
  }

  if (activities.length === 0) {
    return (
      <div className="text-xs text-muted-foreground px-2 py-8">
        No activity captured today yet — configure repos or Claude projects dir
        if you're expecting some.
      </div>
    );
  }

  return (
    <div className="h-[280px] w-full overflow-y-auto overflow-x-hidden rounded-md border border-border/60 bg-card/40">
      <div className="flex flex-col p-1 min-w-0 w-full">
        {activities.map((a) => (
          <ActivityRow
            key={a.id}
            activity={a}
            selected={selectedId === a.id}
            onSelect={onSelect}
            onToggle={onToggle}
          />
        ))}
      </div>
    </div>
  );
}
