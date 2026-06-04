import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderPlus, Trash2 } from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { Config } from "@/lib/types";

export interface ConfigDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: Config | null;
  onSave: (next: Config) => Promise<void>;
}

export function ConfigDialog({
  open,
  onOpenChange,
  config,
  onSave,
}: ConfigDialogProps) {
  const [draft, setDraft] = useState<Config | null>(config);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setDraft(config);
      setError(null);
    }
  }, [open, config]);

  async function pickRepo() {
    try {
      const chosen = await openDialog({ directory: true, multiple: false });
      if (!chosen || typeof chosen !== "string") return;
      setDraft((d) =>
        d
          ? {
              ...d,
              git: {
                repos: d.git.repos.includes(chosen)
                  ? d.git.repos
                  : [...d.git.repos, chosen],
              },
            }
          : d,
      );
    } catch (e: unknown) {
      setError(String(e));
    }
  }

  function removeRepo(path: string) {
    setDraft((d) =>
      d ? { ...d, git: { repos: d.git.repos.filter((p) => p !== path) } } : d,
    );
  }

  function updateProjectsDir(v: string) {
    setDraft((d) =>
      d ? { ...d, claude_code: { projects_dir: v } } : d,
    );
  }

  async function handleSave() {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      await onSave(draft);
      onOpenChange(false);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>Happycampr Carousels settings</DialogTitle>
          <DialogDescription>
            Local-only. Changes are written to{" "}
            <span className="font-mono text-xs">
              ~/Library/Application Support/happycampr-carousels/config.toml
            </span>
            .
          </DialogDescription>
        </DialogHeader>

        {!draft ? (
          <div className="flex flex-col gap-3 py-4 text-sm">
            <p className="text-destructive">
              Could not load the current config. This usually means the config
              file is missing or corrupt.
            </p>
            <p className="text-xs text-muted-foreground">
              Try: close this dialog, restart the app. A missing file will be
              re-created with defaults on next launch. If you see this again,
              inspect{" "}
              <span className="font-mono">
                ~/Library/Application Support/happycampr-carousels/config.toml
              </span>
              .
            </p>
          </div>
        ) : (
        <div className="flex flex-col gap-5 py-2">
          <section className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <h3 className="text-xs uppercase tracking-wider text-muted-foreground">
                Git repositories
              </h3>
              <Button
                variant="outline"
                size="xs"
                onClick={pickRepo}
                disabled={!draft || saving}
              >
                <FolderPlus className="size-3" data-icon="inline-start" />
                Add repo
              </Button>
            </div>
            <ScrollArea className="h-[140px] rounded-md border border-border/60 bg-card/40">
              <div className="flex flex-col p-1">
                {!draft || draft.git.repos.length === 0 ? (
                  <div className="text-xs text-muted-foreground px-2 py-4">
                    No repos yet. Add one to pull today's commits into the
                    activity list.
                  </div>
                ) : (
                  draft.git.repos.map((path) => (
                    <div
                      key={path}
                      className="flex items-center justify-between gap-2 px-2 py-1.5 rounded hover:bg-muted/30"
                    >
                      <span
                        className="font-mono text-xs truncate"
                        title={path}
                      >
                        {path}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        onClick={() => removeRepo(path)}
                        aria-label={`Remove ${path}`}
                        disabled={saving}
                      >
                        <Trash2 className="size-3" />
                      </Button>
                    </div>
                  ))
                )}
              </div>
            </ScrollArea>
          </section>

          <section className="flex flex-col gap-2">
            <h3 className="text-xs uppercase tracking-wider text-muted-foreground">
              Claude Code projects directory
            </h3>
            <input
              type="text"
              value={draft?.claude_code.projects_dir ?? ""}
              onChange={(e) => updateProjectsDir(e.currentTarget.value)}
              className="font-mono text-xs bg-card/40 border border-border rounded-md h-8 px-2"
              spellCheck={false}
              disabled={!draft || saving}
            />
            <p className="text-[11px] text-muted-foreground">
              Usually{" "}
              <span className="font-mono">~/.claude/projects</span>.
            </p>
          </section>

          {error && (
            <div className="text-xs text-destructive border border-destructive/40 rounded-md p-2 bg-destructive/10">
              {error}
            </div>
          )}
        </div>
        )}

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={!draft || saving}>
            {saving ? "Saving…" : "Save settings"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
