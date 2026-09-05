import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import { placeFloating, type CursorPosition as Position } from "./floatingGeometry";

/** Coalesce cursor samples and clamp the measured content using the webview's actual DPI. */
export function useFloatingPosition(event: string) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let latest: Position | null = null;
    let frame = 0;
    let disposed = false;
    let received = false;
    let left = false;
    let idle: ReturnType<typeof setTimeout> | undefined;
    const paint = () => {
      frame = 0;
      if (!latest || disposed) return;
      const placed = placeFloating(latest,
        { width: window.innerWidth, height: window.innerHeight, scale: window.devicePixelRatio },
        { width: element.offsetWidth, height: element.offsetHeight }, left);
      left = placed.left;
      const { x, y } = placed;
      element.dataset.side = left ? "left" : "right";
      element.style.transform = `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`;
      element.style.visibility = "visible";
    };
    const schedule = () => { if (!frame && !disposed) frame = requestAnimationFrame(paint); };
    const accept = (position: Position) => {
      if (![position.x, position.y, position.originX, position.originY].every(Number.isFinite)) return;
      if (latest && (position.sequence ?? 0) < (latest.sequence ?? 0)) return;
      latest = position;
      element.dataset.moving = "true";
      clearTimeout(idle);
      idle = setTimeout(() => { element.dataset.moving = "false"; }, 130);
      schedule();
    };
    const unlisten = listen<Position>(event, ({ payload }) => { received = true; accept(payload); });
    void unlisten.then(async () => {
      const snapshot = await invoke<Position | null>("floating_position");
      if (!disposed && !received && snapshot) accept(snapshot);
    }).catch(() => { /* A later native sample can recover readiness. */ });
    const observer = new ResizeObserver(schedule);
    observer.observe(element);
    window.addEventListener("resize", schedule);
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      clearTimeout(idle);
      observer.disconnect();
      window.removeEventListener("resize", schedule);
      void unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [event]);
  return ref;
}
