import { useCallback, useEffect, useState } from "react";
import { Settings } from "lucide-react";

import { CarouselsView } from "@/components/CarouselsView";
import { ConfigDialog } from "@/components/ConfigDialog";
import { LogView } from "@/components/LogView";
import { TopTabs, type TopTabValue } from "@/components/TopTabs";
import { Button } from "@/components/ui/button";
import { loadConfig, saveConfig } from "@/lib/tauri";
import type { Config } from "@/lib/types";

import "./styles/globals.css";

export default function App() {
  const [view, setView] = useState<TopTabValue>("log");
  const [config, setConfig] = useState<Config | null>(null);
  const [configOpen, setConfigOpen] = useState(false);

  // Load config once at startup so the dialog opens with populated values.
  useEffect(() => {
    loadConfig()
      .then(setConfig)
      .catch((e: unknown) => {
        console.error("cantalog: loadConfig failed", e);
      });
  }, []);

  const handleSaveConfig = useCallback(async (next: Config) => {
    await saveConfig(next);
    setConfig(next);
  }, []);

  return (
    <div className="min-h-screen bg-background text-foreground font-sans">
      <main className="mx-auto w-full max-w-[640px] p-6 flex flex-col gap-6">
        <header className="flex items-center justify-between">
          <TopTabs value={view} onChange={setView} />
          <div className="flex items-center gap-2">
            <span
              className="font-mono text-[10px] text-muted-foreground tracking-tight select-all"
              title="Git SHA of the commit this build was compiled from"
            >
              {__GIT_SHA__}
            </span>
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

        {view === "log" ? (
          <LogView
            configOpen={configOpen}
            onOpenConfig={() => setConfigOpen(true)}
          />
        ) : (
          <CarouselsView />
        )}
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
