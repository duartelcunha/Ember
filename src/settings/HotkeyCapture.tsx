import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { ipc, type HotkeySlot, type HotkeyVerdict } from "@/lib/ipc";

/** O webview corre o mesmo JS nas duas plataformas, mas a tecla `Meta` nao e a mesma coisa:
 *  no macOS e o Command (o modificador de atalhos), no Windows e a tecla Windows. Sem isto,
 *  carregar Win+Espaco no Windows era gravado como Ctrl+Espaco, um atalho completamente
 *  diferente do que a pessoa acabou de premir. */
const IS_MAC = /Mac|iPhone|iPad/.test(navigator.userAgent);

/** Traduz um KeyboardEvent para o formato de atalho do Tauri (ex: "CmdOrCtrl+Shift+Space").
 *  Devolve `null` enquanto so ha modificadores premidos (ainda nao ha tecla "principal"). */
function toAccelerator(e: KeyboardEvent): string | null {
  const mods = modifiersOf(e);

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

/** Os modificadores premidos, no vocabulario do Tauri e com a semantica certa por plataforma. */
function modifiersOf(e: KeyboardEvent): string[] {
  const mods: string[] = [];
  if (IS_MAC) {
    // No macOS o modificador de atalhos e o Command (`metaKey`); o Control e uma tecla a parte
    // e tem os seus proprios atalhos de sistema.
    if (e.metaKey) mods.push("CmdOrCtrl");
    if (e.ctrlKey) mods.push("Control");
  } else {
    if (e.ctrlKey) mods.push("CmdOrCtrl");
    // A tecla Windows. O Tauri chama-lhe "Super".
    if (e.metaKey) mods.push("Super");
  }
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  return mods;
}

/** A frase a mostrar quando a combinacao nao serve. Diz sempre QUEM a esta a usar: um
 *  "invalido" generico deixa a pessoa a adivinhar qual das combinacoes tentar a seguir. */
function refusal(accel: string, v: HotkeyVerdict): string | null {
  switch (v.kind) {
    case "available":
      return null;
    case "reserved_by_os":
      return `${accel} is already used by ${v.owner}. Try another.`;
    case "used_by_ember":
      return `${accel} is already your ${v.slot} shortcut.`;
    case "incomplete":
      return "Hold your modifiers and press a key.";
    case "needs_modifier":
      return `${accel} on its own would take that key from every app. Add Ctrl, Alt or Shift.`;
    case "clashes_with_picker":
      return `The project picker uses ${v.key} to navigate, so ${accel} would close the list instead of moving in it. Pick a combination without it.`;
  }
}

/** Capturador de atalho: em vez de escrever o texto, clicas "Set shortcut", carregas a combinacao
 *  no teclado e ela fica gravada (como no VS Code). Mostra o atalho atual e um preview ao vivo.
 *
 *  Uma combinacao ja ocupada e RECUSADA aqui, antes de ser gravada: a caixa fica vermelha, diz
 *  quem a esta a usar, e a captura continua a espera de outra. Antes gravava-se e so um toast
 *  dizia que tinha falhado, o que deixava a pessoa sem saber o que tentar a seguir. */
export function HotkeyCapture({
  value,
  slot,
  onCommit,
  clearable = false,
  ariaLabel,
}: {
  value: string;
  /** Qual dos tres atalhos, para o check saber com o que comparar. */
  slot: HotkeySlot;
  /** Grava e devolve `null` em sucesso ou a mensagem de erro. A mensagem volta para o MESMO
   *  alerta inline do pre-check: um canal de erro so, em vez de "inline para recusas, toast
   *  para falhas de registo", que deixava a caixa a mostrar o valor antigo como se nada fosse. */
  onCommit: (accel: string) => Promise<string | null>;
  /** Atalhos opcionais (os de modo) podem ficar vazios, e vazio quer dizer "nao registes". */
  clearable?: boolean;
  ariaLabel?: string;
}) {
  const [capturing, setCapturing] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
        setCapturing(false);
        setPreview(null);
        setError(null);
        return;
      }
      const accel = toAccelerator(e);
      if (!accel) {
        // So modificadores ainda: mostra o preview ao vivo ("CmdOrCtrl+Shift+...").
        const mods = modifiersOf(e);
        setError(null);
        setPreview(mods.length ? mods.join("+") + "+…" : "…");
        return;
      }
      // Combinacao completa. Pergunta ao nucleo se pode ser gravada ANTES de a gravar.
      setError(null);
      setPreview(accel);
      setChecking(true);
      // Gravacao com o erro de volta AQUI: se o registo real falhar (outra app apanhou a
      // combinacao entre o check e o set), a captura reabre com a mensagem, em vez de um toast
      // passageiro e a caixa a fingir que nada aconteceu.
      const commit = () =>
        onCommit(accel).then((msg) => {
          if (msg) {
            setError(msg);
            setPreview(accel);
            setCapturing(true);
          }
        });
      ipc
        .checkHotkey(slot, accel)
        .then((verdict) => {
          const msg = refusal(accel, verdict);
          if (msg) {
            // Recusada: fica em captura, a vermelho, a espera de outra combinacao. O combo
            // rejeitado FICA visivel (riscado): apaga-lo deixava o erro a falar de uma
            // combinacao que ja nao se via em lado nenhum.
            setError(msg);
            return;
          }
          setCapturing(false);
          setPreview(null);
          setError(null);
          return commit();
        })
        .catch(() => {
          // Fora do Tauri, ou o check em si falhou. Nao bloqueia a gravacao por causa de uma
          // verificacao que nao correu: o `set_hotkey` volta a validar e desfaz se for preciso.
          setCapturing(false);
          setPreview(null);
          setError(null);
          return commit();
        })
        .finally(() => setChecking(false));
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, onCommit, slot]);

  const startCapture = () => {
    setError(null);
    setPreview(null);
    setCapturing(true);
  };

  const boxClass = error
    ? "border-[color:var(--color-error)] bg-surface-1 text-fg"
    : capturing
      ? "border-[color:var(--border-accent)] bg-surface-1 text-fg-muted"
      : "border-[color:var(--border-subtle)] bg-surface-2 text-fg";

  // Sair da captura sem gravar limpa TUDO: sem isto o Cancel deixava o erro vermelho da
  // tentativa anterior preso na caixa, sem captura ativa que o explicasse.
  const cancelCapture = () => {
    setCapturing(false);
    setPreview(null);
    setError(null);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <div
          role="button"
          tabIndex={0}
          aria-label={ariaLabel}
          aria-invalid={error ? true : undefined}
          onClick={() => {
            if (!capturing) startCapture();
          }}
          onKeyDown={(e) => {
            if (!capturing && (e.key === "Enter" || e.key === " ")) {
              e.preventDefault();
              startCapture();
            }
          }}
          className={`flex h-9 flex-1 cursor-pointer items-center gap-2 rounded-sm border px-3 font-mono text-sm ${boxClass}`}
        >
          {capturing ? (
            <>
              {preview ? (
                <span className={error ? "text-fg-muted line-through" : undefined}>
                  {preview}
                </span>
              ) : (
                <span className="text-fg-muted">Press your shortcut…</span>
              )}
              {checking && (
                <span className="flex items-center gap-1.5 font-sans text-xs text-fg-muted">
                  <Spinner size={12} /> checking…
                </span>
              )}
              {!checking && !preview && (
                <span className="ml-auto font-sans text-xs text-fg-muted">Esc cancels</span>
              )}
            </>
          ) : (
            value || "Not set"
          )}
        </div>
        {capturing ? (
          <Button variant="ghost" onClick={cancelCapture}>
            Cancel
          </Button>
        ) : (
          <>
            <Button variant="primary" onClick={startCapture}>
              Set shortcut
            </Button>
            {clearable && value && (
              <Button variant="ghost" onClick={() => void onCommit("")}>
                Clear
              </Button>
            )}
          </>
        )}
      </div>
      {error && (
        <p role="alert" className="text-xs text-[color:var(--color-error)]">
          {error}
        </p>
      )}
    </div>
  );
}
