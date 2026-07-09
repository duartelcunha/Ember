import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, X } from "@phosphor-icons/react";

/** Barra de titulo custom (a janela e decorations:false). Seamless: sem faixa preta nativa, so
 *  uma faixa arrastavel que se funde com o painel, com minimizar e fechar (sem maximizar). O
 *  fechar ESCONDE para a tray (o handler nativo em Rust trata do CloseRequested; aqui chamamos
 *  hide diretamente, que e o mesmo efeito e evita o flicker de disparar o evento). */
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
        onClick={() => win.hide()}
        aria-label="Close"
        className="group grid h-7 w-9 place-items-center rounded-md text-fg-muted transition-colors hover:bg-[color:var(--color-error)] hover:text-white"
      >
        <X size={15} weight="bold" />
      </button>
    </div>
  );
}
