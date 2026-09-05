import { useLayoutEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import { placeFloating, placeOrb, placeLabels, geometryReady, type CursorPosition as Position } from "./floatingGeometry";

/** Coalesce cursor samples and clamp the measured content using the webview's actual DPI. */
export function useFloatingPosition(event: string, mode: "surface" | "orb" | "labels" = "surface") {
  const ref = useRef<HTMLDivElement>(null);
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const reposition = useRef<(() => void) | null>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    let latest: Position | null = null;
    let frame = 0;
    let disposed = false;
    let received = false;
    let left = false;
    const paint = () => {
      cancelAnimationFrame(frame);
      frame = 0;
      if (!latest || disposed) return;
      const view = { width: window.innerWidth, height: window.innerHeight, scale: window.devicePixelRatio };
      if (!geometryReady(latest, view)) { element.style.visibility = "hidden"; return; }
      const placed = modeRef.current === "orb" ? placeOrb(latest, view, left) : (modeRef.current === "labels" ? placeLabels : placeFloating)(
        latest,
        view, { width: element.offsetWidth, height: element.offsetHeight }, left);
      left = placed.left;
      const { x, y } = placed;
      element.dataset.side = left ? "left" : "right";
      element.style.transform = `translate3d(${Math.round(x * view.scale) / view.scale}px, ${Math.round(y * view.scale) / view.scale}px, 0)`;
      element.style.visibility = "visible";
    };
    const schedule = () => { if (!frame && !disposed) frame = requestAnimationFrame(paint); };
    const accept = (position: Position) => {
      if (![position.x, position.y, position.originX, position.originY].every(Number.isFinite)) return;
      if (latest && (position.sequence ?? 0) < (latest.sequence ?? 0)) return;
      if ((position.generation ?? 0) < (latest?.generation ?? 0)) return;
      latest = position;
      if (!geometryReady(position, { width: innerWidth, height: innerHeight, scale: devicePixelRatio })) element.style.visibility = "hidden";
      schedule();
    };
    const unlisten = listen<Position>(event, ({ payload }) => { received = true; accept(payload); });
    void unlisten.then(async () => {
      const snapshot = await invoke<Position | null>("floating_position");
      if (!disposed && !received && snapshot) accept(snapshot);
    }).catch(() => { /* A later native sample can recover readiness. */ });
    // Size changes must be clamped before the same frame is presented. Deferring
    // a ResizeObserver callback to another frame exposes the old position.
    reposition.current = paint;
    const observer = new ResizeObserver(paint);
    observer.observe(element);
    window.addEventListener("resize", paint);
    return () => {
      disposed = true;
      reposition.current = null;
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", paint);
      void unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [event]);
  // React can replace a small pill with a wide row without a cursor sample.
  useLayoutEffect(() => { reposition.current?.(); });
  return ref;
}
