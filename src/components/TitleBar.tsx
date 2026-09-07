import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "@phosphor-icons/react";

/** Custom title bar (the window is decorations:false). Seamless: no native black strip, just a
 *  draggable band that blends into the panel, with minimise and close (no maximise). Close goes
 *  through the native CloseRequested handler in Rust, which saves the window geometry and then
 *  hides to the tray. It used to call hide() directly; that skipped the handler, so the size and
 *  position the user had just chosen were never persisted. */
export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <div
      data-tauri-drag-region
      className="fixed inset-x-0 top-0 z-50 flex h-9 items-center justify-end gap-0.5 px-1.5 select-none"
    >
      {/* Botoes: nao arrastaveis (o data-tauri-drag-region no pai torna o resto arrastavel). */}
      <button
        onClick={() => win.minimize()}
        aria-label="Minimize"
        className="grid h-7 w-9 place-items-center rounded-md text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg"
      >
        <Minus size={15} weight="bold" />
      </button>
      <button
        onClick={() => win.close()}
        aria-label="Close"
        className="group grid h-7 w-9 place-items-center rounded-md text-fg-muted transition-colors hover:bg-[color:var(--color-error)] hover:text-white"
      >
        <X size={15} weight="bold" />
      </button>
    </div>
  );
}
