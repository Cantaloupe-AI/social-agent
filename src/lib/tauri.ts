import { invoke } from "@tauri-apps/api/core";
import type { Activity, Config, Entry } from "./types";

export async function fetchTodayActivities(): Promise<Activity[]> {
  return invoke<Activity[]>("fetch_today_activities");
}

export async function loadTodayEntry(): Promise<Entry | null> {
  return invoke<Entry | null>("load_today_entry");
}

export async function saveEntry(
  activities: Activity[],
  thoughts: string,
): Promise<void> {
  await invoke<void>("save_entry", { activities, thoughts });
}

export async function loadConfig(): Promise<Config> {
  return invoke<Config>("load_config");
}

export async function saveConfig(config: Config): Promise<void> {
  await invoke<void>("save_config", { config });
}

export async function quitApp(): Promise<void> {
  await invoke<void>("quit_app");
}
