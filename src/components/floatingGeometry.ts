export interface CursorPosition { sequence?: number; generation?: number; ready?: boolean; scale?: number; width?: number; height?: number; x: number; y: number; originX: number; originY: number }
export interface Viewport { width: number; height: number; scale: number }
// Anchor the visible ring beside the standard arrow's right diagonal. This is a
// logical offset from the hotspot, not SVG-box padding.
//
// The ring is a diamond, so its lower-left edge already runs at the arrow's own
// ~45 degrees: the two are parallel by construction and the gap between them is
// constant. What was wrong was the size of that gap. At 14 the measured
// clearance was 3.3px at every point along the edge, and the heat glow (a soft
// circle centred on the ring) washed across the arrow itself, so the pair read
// as one smudge rather than a mark set beside the pointer. 19 puts it at 6.8px,
// which is air you can see without the ring drifting away from the cursor.
//
// `y` is where the content's CENTRE sits relative to the hotspot, not its top
// edge. Aligning tops made every surface hang below the pointer, and worse, by
// an amount that grew with its height: the 15px ring sat 7px low, a one-line
// result surface 14px, a two-line one 23px. 2 puts the centre just inside the
// arrow's head, so the pair reads as one object whatever the surface holds.
//
// The same offset anchors the result surface, because the morph grows it from
// the ring's own 15px corner: a different gap there would make the surface jump
// at the instant it takes over. Custom pointer artwork can differ.
export const CURSOR_GAP = { x: 19, y: 2 };
type PlacementOptions = { gap?: { x: number; y: number }; preserveSide?: boolean; centreY?: boolean };

/** Convert physical cursor coordinates once, then place measured logical content. */
export function placeFloating(cursor: CursorPosition, view: Viewport, content: { width: number; height: number }, wasLeft: boolean, { gap = { x: 14, y: 18 }, preserveSide = false, centreY = false }: PlacementOptions = {}) {
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
  // Menus open below the cursor and keep `gap.y` as a top offset. Surfaces pinned to the
  // pointer centre on it instead, so their height cannot drag them downwards.
  const top = centreY ? cursorY + gap.y - height / 2 : cursorY + gap.y;
  const y = Math.max(margin, Math.min(top, view.height - height - margin));
  return { x, y, left };
}

/** The drawing pixel of the mark. 3 is the size everyone has unless they change it. */
export const ORB_PX = 3;

/**
 * The visible ink of the mark inside its 40px SVG, for a given drawing pixel.
 *
 * Not the SVG box: the artwork is a 5x5 grid of `px` cells sitting in one corner of a canvas
 * three times its size, and it is the ink that has to land beside the cursor. The right edge and
 * the top stay put across sizes, so the clearance from the arrow measured into `CURSOR_GAP` does
 * not depend on how big the mark is. Every size fits the same 40px canvas: 20px of ink at the
 * largest step still ends at 37.
 */
export function orbInk(px: number = ORB_PX) {
  const side = px * 5;
  return { x: 37 - side, y: 2, width: side, height: side };
}

/** The default ink box, for callers that never offered a size. */
export const ORB_INK = orbInk(ORB_PX);
export function geometryReady(cursor: CursorPosition, view: Viewport) {
  return cursor.ready !== false && (cursor.scale === undefined ||
    (Math.abs(cursor.scale - view.scale) < 0.01 &&
     Math.abs((cursor.width ?? 0) / view.scale - view.width) <= 2 &&
     Math.abs((cursor.height ?? 0) / view.scale - view.height) <= 2));
}
export function placeOrb(cursor: CursorPosition, view: Viewport, wasLeft: boolean, preserveSide = false, px: number = ORB_PX) {
  const ink = orbInk(px);
  const cx = (cursor.x - cursor.originX) / view.scale;
  const cy = (cursor.y - cursor.originY) / view.scale;
  let left = wasLeft;
  if (!left && cx + CURSOR_GAP.x + ink.width > view.width - 4) left = true;
  else if (left && !preserveSide && cx + CURSOR_GAP.x + ink.width < view.width - 36) left = false;
  const inkX = Math.max(4, Math.min(left ? cx - CURSOR_GAP.x - ink.width : cx + CURSOR_GAP.x, view.width - ink.width - 4));
  const inkY = Math.max(4, Math.min(cy + CURSOR_GAP.y - ink.height / 2, view.height - ink.height - 4));
  return { x: inkX - ink.x, y: inkY - ink.y, left };
}

export function placeLabels(cursor: CursorPosition, view: Viewport, content: { width: number; height: number }, wasLeft: boolean, px: number = ORB_PX) {
  const ink = orbInk(px);
  const cx = (cursor.x - cursor.originX) / view.scale;
  const cy = (cursor.y - cursor.originY) / view.scale;
  const positioned = placeFloating(cursor, view, content, wasLeft, { gap: CURSOR_GAP });
  // Labels clear both the cursor and the ring, including at the bottom edge.
  const inkY = placeOrb(cursor, view, wasLeft, false, px).y + ink.y;
  const below = Math.max(8, cy + ink.height + 11, inkY + ink.height + 8);
  const y = below + content.height <= view.height - 8 ? below : Math.max(8, Math.min(cy - 8, inkY - 8) - content.height);
  return { ...positioned, x: Math.max(8, Math.min(positioned.left ? cx - CURSOR_GAP.x - content.width : cx + CURSOR_GAP.x, view.width - content.width - 8)), y };
}
