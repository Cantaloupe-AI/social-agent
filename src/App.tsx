import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Settings } from "lucide-react";

import { ActivityList } from "@/components/ActivityList";
import { ConfigDialog } from "@/components/ConfigDialog";
import { SaveButton, type SaveState } from "@/components/SaveButton";
import { ThoughtsField } from "@/components/ThoughtsField";
import { Button } from "@/components/ui/button";
import { useActivities } from "@/hooks/useActivities";
import { useKeyboard } from "@/hooks/useKeyboard";
import { useTodayEntry } from "@/hooks/useTodayEntry";
import { mergeEntryWithFetched } from "@/lib/merge";
import { loadConfig, saveConfig, saveEntry } from "@/lib/tauri";
import { formatFriendlyDate } from "@/lib/time";
import type { Config } from "@/lib/types";

import "./styles/globals.css";

export default function App() {
  const {
    activities,
    setActivities,
    toggle,
    loading: activitiesLoading,
    error: activitiesError,
  } = useActivities();
  const { entry, loading: entryLoading } = useTodayEntry();

  const [thoughts, setThoughts] = useState("");
  const [initialThoughts, setInitialThoughts] = useState("");
  const [mergedApplied, setMergedApplied] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [config, setConfig] = useState<Config | null>(null);
  const [configOpen, setConfigOpen] = useState(false);

  const loading = activitiesLoading || entryLoading;

  // Merge the saved entry into the fetched activities once, after both loads finish.
  useEffect(() => {
    if (loading || mergedApplied) return;
    setActivities(mergeEntryWithFetched(entry, activities));
    if (entry) {
      setThoughts(entry.thoughts);
      setInitialThoughts(entry.thoughts);
    }
    setMergedApplied(true);
  }, [loading, mergedApplied, entry, activities, setActivities]);

  // Keep a valid selection as activities load/change.
  useEffect(() => {
    if (activities.length === 0) {
      setSelectedId(null);
      return;
    }
    if (!selectedId || !activities.some((a) => a.id === selectedId)) {
      setSelectedId(activities[0].id);
    }
  }, [activities, selectedId]);

  // Load config once at startup so the dialog opens with populated values.
  useEffect(() => {
    loadConfig()
      .then(setConfig)
      .catch((e: unknown) => {
        console.error("cantalog: loadConfig failed", e);
      });
  }, []);

  const moveSelection = useCallback(
    (delta: 1 | -1) => {
      setSelectedId((prev) => {
        if (activities.length === 0) return null;
        const idx = prev ? activities.findIndex((a) => a.id === prev) : -1;
        const base = idx >= 0 ? idx : 0;
        const next = Math.max(0, Math.min(activities.length - 1, base + delta));
        return activities[next].id;
      });
    },
    [activities],
  );

  const dirty = useMemo(
    () => thoughts !== initialThoughts,
    [thoughts, initialThoughts],
  );

  const handleSave = useCallback(async () => {
    if (saveState === "saving") return;
    if (configOpen) return;
    setSaveState("saving");

    const noteText = thoughts.trim();
    let nextActivities = activities;
    if (noteText.length > 0) {
      const nowIso = new Date().toISOString();
      const note: (typeof activities)[number] = {
        id: `note:${nowIso}`,
        source: "note",
        timestamp: nowIso,
        summary: noteText,
        chip: "note",
        included: true,
      };
      nextActivities = [note, ...activities];
      setActivities(nextActivities);
    }

    try {
      const included = nextActivities.filter((a) => a.included);
      // Thoughts field has been converted to a note row; save an empty thoughts
      // column so we don't persist the scratch text twice.
      await saveEntry(included, "");
      if (noteText.length > 0) {
        setThoughts("");
        setInitialThoughts("");
      }
      setSaveState("saved");
      setTimeout(() => {
        setSaveState((s) => (s === "saved" ? "idle" : s));
      }, 1200);
    } catch (e: unknown) {
      console.error("cantalog: saveEntry failed", e);
      setSaveState("idle");
    }
  }, [activities, thoughts, saveState, configOpen, setActivities]);

  const handleSaveConfig = useCallback(async (next: Config) => {
    await saveConfig(next);
    setConfig(next);
  }, []);

  const handleEscape = useCallback(() => {
    // Radix dialog already handles Escape to close itself; don't also quit the app.
    if (configOpen) return;
    if (dirty) {
      const proceed = window.confirm(
        "Unsaved thoughts will be lost. Close anyway?",
      );
      if (!proceed) return;
    }
    // Close the window. The backend's RunEvent::WindowEvent::Destroyed handler then
    // calls app.exit(0) so the process terminates cleanly on macOS.
    void getCurrentWindow().close();
  }, [dirty, configOpen]);

  useKeyboard({
    onNext: () => moveSelection(1),
    onPrev: () => moveSelection(-1),
    onToggle: () => {
      if (selectedId) toggle(selectedId);
    },
    onSave: handleSave,
    onOpenConfig: () => setConfigOpen(true),
    onEscape: handleEscape,
  });

  const today = useMemo(() => new Date(), []);

  return (
    <div className="min-h-screen bg-background text-foreground font-sans">
      <main className="mx-auto w-full max-w-[640px] p-6 flex flex-col gap-6">
        <header className="flex items-start justify-between">
          <div>
            <h1 className="text-xl font-semibold tracking-tight">
              {formatFriendlyDate(today)}
            </h1>
            <p className="text-xs text-muted-foreground mt-0.5">
              What happened today?
            </p>
          </div>
          <div className="flex items-center gap-2">
            <span
              className="size-2 rounded-full bg-red-500"
              aria-hidden
              title="build marker"
            />
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setConfigOpen(true)}
              aria-label="Open settings"
            >
              <Settings className="size-4" />
            </Button>
          </div>
        </header>

        {activitiesError && (
          <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
            Could not load activities: {activitiesError}
          </div>
        )}

        <ActivityList
          activities={activities}
          selectedId={selectedId}
          onToggle={toggle}
          onSelect={setSelectedId}
          loading={loading}
        />

        <ThoughtsField value={thoughts} onChange={setThoughts} />

        <SaveButton state={saveState} onSave={handleSave} />
      </main>

      <ConfigDialog
        open={configOpen}
        onOpenChange={setConfigOpen}
        config={config}
        onSave={handleSaveConfig}
      />
    </div>
  );
}
