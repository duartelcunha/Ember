export interface CursorPosition { sequence?: number; generation?: number; ready?: boolean; scale?: number; width?: number; height?: number; x: number; y: number; originX: number; originY: number }
export interface Viewport { width: number; height: number; scale: number }
export const CURSOR_GAP = { x: 18, y: 6 };
type PlacementOptions = { gap?: { x: number; y: number }; preserveSide?: boolean };

/** Convert physical cursor coordinates once, then place measured logical content. */
export function placeFloating(cursor: CursorPosition, view: Viewport, content: { width: number; height: number }, wasLeft: boolean, { gap = { x: 14, y: 18 }, preserveSide = false }: PlacementOptions = {}) {
  const scale = view.scale > 0 && Number.isFinite(view.scale) ? view.scale : 1;
  const cursorX = (cursor.x - cursor.originX) / scale;
  const cursorY = (cursor.y - cursor.originY) / scale;
  const margin = Math.min(8, view.width / 2, view.height / 2);
  const width = Math.min(content.width, Math.max(0, view.width - margin * 2));
  const height = Math.min(content.height, Math.max(0, view.height - margin * 2));
  let left = wasLeft;
  if (!left && cursorX + gap.x + width > view.width - margin) left = true;
  else if (left && !preserveSide && cursorX + gap.x + width < view.width - margin - 32) left = false;
  const x = Math.max(margin, Math.min(left ? cursorX - gap.x - width : cursorX + gap.x, view.width - width - margin));
  const y = Math.max(margin, Math.min(cursorY + gap.y, view.height - height - margin));
  return { x, y, left };
}

// The visible pixel ring in Orb.tsx occupies (22, 2, 15, 15), not its 40px SVG.
export const ORB_INK = { x: 22, y: 2, width: 15, height: 15 };
export function geometryReady(cursor: CursorPosition, view: Viewport) {
  return cursor.ready !== false && (cursor.scale === undefined ||
    (Math.abs(cursor.scale - view.scale) < 0.01 &&
     Math.abs((cursor.width ?? 0) / view.scale - view.width) <= 2 &&
     Math.abs((cursor.height ?? 0) / view.scale - view.height) <= 2));
}
export function placeOrb(cursor: CursorPosition, view: Viewport, wasLeft: boolean, preserveSide = false) {
  const cx = (cursor.x - cursor.originX) / view.scale;
  const cy = (cursor.y - cursor.originY) / view.scale;
  let left = wasLeft;
  if (!left && cx + CURSOR_GAP.x + ORB_INK.width > view.width - 4) left = true;
  else if (left && !preserveSide && cx + CURSOR_GAP.x + ORB_INK.width < view.width - 36) left = false;
  const inkX = Math.max(4, Math.min(left ? cx - CURSOR_GAP.x - ORB_INK.width : cx + CURSOR_GAP.x, view.width - ORB_INK.width - 4));
  const inkY = Math.max(4, Math.min(cy + CURSOR_GAP.y, view.height - ORB_INK.height - 4));
  return { x: inkX - ORB_INK.x, y: inkY - ORB_INK.y, left };
}

export function placeLabels(cursor: CursorPosition, view: Viewport, content: { width: number; height: number }, wasLeft: boolean) {
  const cx = (cursor.x - cursor.originX) / view.scale;
  const cy = (cursor.y - cursor.originY) / view.scale;
  const positioned = placeFloating(cursor, view, content, wasLeft, { gap: CURSOR_GAP });
  // Labels clear both the cursor and the ring, including at the bottom edge.
  const inkY = placeOrb(cursor, view, wasLeft).y + ORB_INK.y;
  const below = Math.max(8, cy + 26, inkY + ORB_INK.height + 8);
  const y = below + content.height <= view.height - 8 ? below : Math.max(8, Math.min(cy - 8, inkY - 8) - content.height);
  return { ...positioned, x: Math.max(8, Math.min(positioned.left ? cx - CURSOR_GAP.x - content.width : cx + CURSOR_GAP.x, view.width - content.width - 8)), y };
}
