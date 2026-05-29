import { useEffect, useRef } from "react";

export interface KeyboardHandlers {
  onNext?: () => void;
  onPrev?: () => void;
  onToggle?: () => void;
  onSave?: () => void;
  onOpenConfig?: () => void;
  onEscape?: () => void;
}

/**
 * Global keyboard shortcuts for Happycampr Carousels:
 * - j / ArrowDown → onNext (move selection down)
 * - k / ArrowUp   → onPrev (move selection up)
 * - x / Space     → onToggle (toggle selected activity)
 * - Cmd/Ctrl+Enter → onSave
 * - Cmd/Ctrl+,    → onOpenConfig
 * - Escape        → onEscape
 *
 * Navigation/toggle keys are suppressed when focus is inside an input or textarea so
 * typing into the thoughts field doesn't hijack j/k/space/x.
 */
export function useKeyboard(handlers: KeyboardHandlers): void {
  const handlersRef = useRef(handlers);
  useEffect(() => {
    handlersRef.current = handlers;
  }, [handlers]);

  useEffect(() => {
    function isTyping(target: EventTarget | null): boolean {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      return (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        target.isContentEditable
      );
    }

    function onKey(e: KeyboardEvent): void {
      const h = handlersRef.current;
      const meta = e.metaKey || e.ctrlKey;

      if (meta && e.key === "Enter") {
        e.preventDefault();
        h.onSave?.();
        return;
      }
      if (meta && e.key === ",") {
        e.preventDefault();
        h.onOpenConfig?.();
        return;
      }
      if (e.key === "Escape") {
        h.onEscape?.();
        return;
      }

      if (isTyping(e.target)) return;

      if (e.key === "j" || e.key === "ArrowDown") {
        e.preventDefault();
        h.onNext?.();
        return;
      }
      if (e.key === "k" || e.key === "ArrowUp") {
        e.preventDefault();
        h.onPrev?.();
        return;
      }
      if (e.key === "x" || e.key === " ") {
        e.preventDefault();
        h.onToggle?.();
        return;
      }
    }

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
