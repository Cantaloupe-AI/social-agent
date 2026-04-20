import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import type { Activity, Config, Entry } from "./types";

// When the frontend is loaded in a plain browser (e.g. `bun run dev` for UI preview)
// there's no Tauri IPC bridge. @tauri-apps/api's invoke() then tries to read
// `window.__TAURI_INTERNALS__.invoke` and throws a cryptic "Cannot read properties
// of undefined (reading 'invoke')". Check up front so the error surfaces an actionable
// message instead.
function isTauri(): boolean {
  return (
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
  );
}

async function call<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      "Cantalog is running in browser preview mode — launch with `bun run tauri dev` to use Tauri commands.",
    );
  }
  return invoke<T>(cmd, args);
}

export async function fetchTodayActivities(): Promise<Activity[]> {
  return call<Activity[]>("fetch_today_activities");
}

export async function loadTodayEntry(): Promise<Entry | null> {
  return call<Entry | null>("load_today_entry");
}

export async function saveEntry(
  activities: Activity[],
  thoughts: string,
): Promise<void> {
  await call<void>("save_entry", { activities, thoughts });
}

export async function loadConfig(): Promise<Config> {
  return call<Config>("load_config");
}

export async function saveConfig(config: Config): Promise<void> {
  await call<void>("save_config", { config });
}

export async function quitApp(): Promise<void> {
  await call<void>("quit_app");
}
