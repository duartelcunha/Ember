import { useLayoutEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import { CURSOR_GAP, ORB_PX, orbInk, placeFloating, placeOrb, placeLabels, geometryReady, type CursorPosition as Position, type Viewport } from "./floatingGeometry";

/**
 * Coalesce cursor samples and clamp the measured content using the webview's actual DPI.
 *
 * `px` is the mark's drawing pixel, which decides the ink box the orb and its labels are placed
 * against. It is a ref rather than a dependency because the size can change between two frames
 * of the same run (the settings window writes it while a refine is on screen) and remounting the
 * observer would drop the cursor stream.
 */
export function useFloatingPosition(event: string, mode: "surface" | "card" | "orb" | "labels" = "surface", px: number = ORB_PX) {
  const ref = useRef<HTMLDivElement>(null);
  const modeRef = useRef(mode);
  modeRef.current = mode;
  const pxRef = useRef(px);
  pxRef.current = px;
  const reposition = useRef<(() => void) | null>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    let latest: Position | null = null;
    let frame = 0;
    let disposed = false;
    let received = false;
    let left = false;
    let painted: Position | null = null;
    let paintedView: Viewport | null = null;
    const paint = () => {
      cancelAnimationFrame(frame);
      frame = 0;
      if (!latest || disposed) return;
      const view = { width: window.innerWidth, height: window.innerHeight, scale: window.devicePixelRatio };
      if (!geometryReady(latest, view)) { element.style.visibility = "hidden"; return; }
      // Enter changes content, not the cursor anchor. Keep the near edge on the
      // same side until pointer movement or a new monitor geometry warrants it.
      const preserveSide = painted?.x === latest.x && painted.y === latest.y
        && painted.originX === latest.originX && painted.originY === latest.originY
        && painted.generation === latest.generation && paintedView?.scale === view.scale
        && paintedView.width === view.width && paintedView.height === view.height;
      const measured = element.getBoundingClientRect();
      const size = { width: measured.width, height: measured.height };
      const placed = modeRef.current === "orb" ? placeOrb(latest, view, left, preserveSide, pxRef.current)
        : modeRef.current === "labels" ? placeLabels(latest, view, size, left, pxRef.current)
        : placeFloating(latest, view, size, left, modeRef.current === "card" ? { gap: CURSOR_GAP, preserveSide, centreY: true } : undefined);
      painted = latest;
      paintedView = view;
      left = placed.left;
      const { x, y } = placed;
      // Snap the edge facing the cursor, not the opposite edge of a variable-width
      // card. Preserve fractional measurements at non-integer display scales.
      const ink = orbInk(pxRef.current);
      const anchorX = modeRef.current === "orb" ? ink.x + (left ? ink.width : 0) : left ? size.width : 0;
      const anchorY = modeRef.current === "orb" ? ink.y : 0;
      element.dataset.side = left ? "left" : "right";
      element.style.transform = `translate3d(${Math.round((x + anchorX) * view.scale) / view.scale - anchorX}px, ${Math.round((y + anchorY) * view.scale) / view.scale - anchorY}px, 0)`;
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
