import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import type {
  Activity,
  Carousel,
  Config,
  Entry,
  Slide,
  SlideFeedback,
  SlideVersion,
} from "./types";

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

export async function listCarousels(): Promise<Carousel[]> {
  return call<Carousel[]>("list_carousels");
}

export async function getCarousel(id: string): Promise<Carousel | null> {
  return call<Carousel | null>("get_carousel", { id });
}

export async function createCarousel(label: string): Promise<Carousel> {
  return call<Carousel>("create_carousel", { label });
}

export async function renameCarousel(id: string, label: string): Promise<void> {
  await call<void>("rename_carousel", { id, label });
}

export async function deleteCarousel(id: string): Promise<void> {
  await call<void>("delete_carousel", { id });
}

export async function listSlides(carouselId: string): Promise<Slide[]> {
  return call<Slide[]>("list_slides", { carouselId });
}

export async function createSlide(carouselId: string): Promise<Slide> {
  return call<Slide>("create_slide", { carouselId });
}

export async function updateSlideContent(
  id: string,
  content: string,
): Promise<void> {
  await call<void>("update_slide_content", { id, content });
}

export async function deleteSlide(id: string): Promise<void> {
  await call<void>("delete_slide", { id });
}

export async function listSlideVersions(slideId: string): Promise<SlideVersion[]> {
  return call<SlideVersion[]>("list_slide_versions", { slideId });
}

export async function listSlideFeedback(
  slideVersionId: string,
): Promise<SlideFeedback[]> {
  return call<SlideFeedback[]>("list_slide_feedback", { slideVersionId });
}

export async function generateCarouselPdf(carouselId: string): Promise<void> {
  await call<void>("generate_carousel_pdf", { carouselId });
}

export async function cancelGeneration(carouselId: string): Promise<void> {
  await call<void>("cancel_generation", { carouselId });
}

export async function openPdf(path: string): Promise<void> {
  await call<void>("open_pdf", { path });
}
