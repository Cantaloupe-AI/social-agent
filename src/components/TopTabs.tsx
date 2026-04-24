import { cn } from "@/lib/utils";

export type TopTabValue = "log" | "slides";

interface TopTabsProps {
  value: TopTabValue;
  onChange: (next: TopTabValue) => void;
}

const TABS: { value: TopTabValue; label: string }[] = [
  { value: "log", label: "Log" },
  { value: "slides", label: "Slides" },
];

export function TopTabs({ value, onChange }: TopTabsProps) {
  return (
    <div
      role="tablist"
      aria-label="View"
      className="inline-flex items-center gap-1 rounded-lg bg-muted/40 p-1 text-sm"
    >
      {TABS.map((tab) => {
        const active = tab.value === value;
        return (
          <button
            key={tab.value}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(tab.value)}
            className={cn(
              "rounded-md px-3 py-1 font-medium transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
              active
                ? "bg-background text-foreground shadow-sm ring-1 ring-border"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
