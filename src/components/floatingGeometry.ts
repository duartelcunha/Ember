export interface CursorPosition { sequence?: number; x: number; y: number; originX: number; originY: number }
export interface Viewport { width: number; height: number; scale: number }

/** Convert physical cursor coordinates once, then place measured logical content. */
export function placeFloating(cursor: CursorPosition, view: Viewport, content: { width: number; height: number }, wasLeft: boolean) {
  const scale = view.scale > 0 && Number.isFinite(view.scale) ? view.scale : 1;
  const cursorX = (cursor.x - cursor.originX) / scale;
  const cursorY = (cursor.y - cursor.originY) / scale;
  const margin = Math.min(8, view.width / 2, view.height / 2);
  const width = Math.min(content.width, Math.max(0, view.width - margin * 2));
  const height = Math.min(content.height, Math.max(0, view.height - margin * 2));
  let left = wasLeft;
  if (!left && cursorX + 14 + width > view.width - margin) left = true;
  else if (left && cursorX + 14 + width < view.width - margin - 32) left = false;
  const x = Math.max(margin, Math.min(left ? cursorX - 14 - width : cursorX + 14, view.width - width - margin));
  const y = Math.max(margin, Math.min(cursorY + 18, view.height - height - margin));
  return { x: Math.round(x), y: Math.round(y), left };
}
