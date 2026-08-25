import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";

/** Traduz um KeyboardEvent para o formato de atalho do Tauri (ex: "CmdOrCtrl+Shift+Space").
 *  Devolve `null` enquanto so ha modificadores premidos (ainda nao ha tecla "principal"). */
function toAccelerator(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  // CmdOrCtrl cobre Ctrl no Windows/Linux e Cmd no macOS (o mesmo binding, portavel).
  if (e.ctrlKey || e.metaKey) mods.push("CmdOrCtrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");

  // A tecla principal, a partir de `event.code` (independente do layout/idioma do teclado).
  const code = e.code;
  let key: string | null = null;
  if (code.startsWith("Key")) key = code.slice(3); // KeyA -> A
  else if (code.startsWith("Digit")) key = code.slice(5); // Digit1 -> 1
  else if (code.startsWith("Numpad")) key = code; // mantem Numpad* (o Tauri aceita)
  else if (/^F\d{1,2}$/.test(code)) key = code; // F1..F24
  else {
    // Teclas nomeadas comuns.
    const named: Record<string, string> = {
      Space: "Space",
      Enter: "Enter",
      Tab: "Tab",
      Backspace: "Backspace",
      Escape: "Escape",
      ArrowUp: "Up",
      ArrowDown: "Down",
      ArrowLeft: "Left",
      ArrowRight: "Right",
      Home: "Home",
      End: "End",
      PageUp: "PageUp",
      PageDown: "PageDown",
      Insert: "Insert",
      Delete: "Delete",
      Minus: "-",
      Equal: "=",
      BracketLeft: "[",
      BracketRight: "]",
      Backslash: "\\",
      Semicolon: ";",
      Quote: "'",
      Comma: ",",
      Period: ".",
      Slash: "/",
      Backquote: "`",
    };
    key = named[code] ?? null;
  }

  // So modificadores (sem tecla principal) -> ainda incompleto.
  if (!key) return null;
  return [...mods, key].join("+");
}

/** Capturador de atalho: em vez de escrever o texto, clicas "Set shortcut", carregas a combinacao
 *  no teclado e ela fica gravada (como no VS Code). Mostra o atalho atual e um preview ao vivo. */
export function HotkeyCapture({
  value,
  onCommit,
  clearable = false,
  ariaLabel,
}: {
  value: string;
  onCommit: (accel: string) => Promise<void>;
  /** Atalhos opcionais (os de modo) podem ficar vazios, e vazio quer dizer "nao registes". */
  clearable?: boolean;
  ariaLabel?: string;
}) {
  const [capturing, setCapturing] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturing(false);
        setPreview(null);
        return;
      }
      const accel = toAccelerator(e);
      if (accel) {
        // Combinacao completa: grava e sai do modo de captura.
        setCapturing(false);
        setPreview(null);
        onCommit(accel);
      } else {
        // So modificadores ainda: mostra o preview ao vivo ("CmdOrCtrl+Shift+...").
        const mods: string[] = [];
        if (e.ctrlKey || e.metaKey) mods.push("CmdOrCtrl");
        if (e.altKey) mods.push("Alt");
        if (e.shiftKey) mods.push("Shift");
        setPreview(mods.length ? mods.join("+") + "+…" : "…");
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, onCommit]);

  return (
    <div className="flex items-center gap-2">
      <div
        ref={boxRef}
        aria-label={ariaLabel}
        className={`flex h-9 flex-1 items-center rounded-sm border px-3 font-mono text-sm ${
          capturing
            ? "border-[color:var(--border-accent)] bg-surface-1 text-fg-muted"
            : "border-[color:var(--border-subtle)] bg-surface-2 text-fg"
        }`}
      >
        {capturing ? preview ?? "Press your shortcut…" : value || "Not set"}
      </div>
      {capturing ? (
        <Button variant="ghost" onClick={() => setCapturing(false)}>
          Cancel
        </Button>
      ) : (
        <>
          <Button variant="primary" onClick={() => setCapturing(true)}>
            Set shortcut
          </Button>
          {clearable && value && (
            <Button variant="ghost" onClick={() => onCommit("")}>
              Clear
            </Button>
          )}
        </>
      )}
    </div>
  );
}
