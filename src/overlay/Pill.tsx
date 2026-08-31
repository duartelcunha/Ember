import { m } from "motion/react";
import { WarningCircle, Cursor, Check } from "@phosphor-icons/react";

type Kind = "error" | "hint" | "success";

const ICON = {
  error: <WarningCircle weight="fill" size={14} />,
  hint: <Cursor weight="fill" size={14} />,
  success: <Check weight="bold" size={14} />,
};

/** Pilha de feedback junto ao cursor (erro/hint/sucesso). A bolha (com backdrop-filter) faz
 *  SO fade: mexer-lhe em transform re-amostra o fundo desfocado cada frame. O movimento fica
 *  no conteudo interno (icone+texto), que nao tem blur, para um enter fluido a 120fps. */
export function Pill({ kind, text }: { kind: Kind; text: string }) {
  const color =
    kind === "error"
      ? "var(--color-error)"
      : kind === "success"
        ? "var(--color-orb-accent)"
        : "var(--color-fg-muted)";
  return (
    <m.div
      // Sem `max-w`: a bolha mede-se pelo texto. Com um teto, uma frase como "Enter to apply ·
      // Esc to keep original" partia-se em duas linhas e a pilula passava a ser um paragrafo
      // flutuante em vez de uma etiqueta. A janela do overlay e transparente e ignora cliques,
      // por isso deixa-la respirar em largura nao custa nada a ninguem.
      // A janela esta ancorada no MEIO DO PONTEIRO (uma ancora so para as duas fases, ver
      // `orb_target` em lib.rs) e nao se reposiciona quando a fase muda. Logo e esta margem que
      // poe a pilula ao LADO do cursor em vez de por cima dele. A conta, a escala 1: a area de
      // conteudo comeca em `cursor - 14`, portanto 40px poem a borda esquerda em `cursor + 26`.
      // Como a seta ocupa ~12px a direita do hotspot, ficam ~14px de folga limpa entre as duas,
      // que e o que faz o espacamento parecer escolhido em vez de acidental.
      // ESPELHADO em `PILL_MARGIN_X` (lib.rs), que clampa a pilula ao ecra por esta margem.
      className="ember-bubble ml-10 flex w-fit items-center gap-1.5 px-2.5 py-1.5"
      // Segundo tempo do morph: a faisca colapsou para o centro (exit do Orb) e a pilula
      // ACENDE a partir desse ponto (origem a esquerda, scale com spring curto). E uma
      // entrada one-shot de ~200ms: re-amostrar o backdrop-filter durante esses frames e
      // barato; o que continua proibido e transform CONTINUO na bolha (o seguimento do
      // cursor a 120fps re-amostrava o blur a cada frame).
      style={{ borderRadius: 12, willChange: "opacity, transform", transformOrigin: "left center" }}
      initial={{ opacity: 0, scale: 0.55, x: -14 }}
      animate={{ opacity: 1, scale: 1, x: 0 }}
      exit={{ opacity: 0, transition: { duration: 0.3, ease: [0.22, 1, 0.36, 1] } }}
      transition={{
        opacity: { duration: 0.16, ease: "easeOut" },
        scale: { type: "spring", stiffness: 520, damping: 30 },
        x: { type: "spring", stiffness: 520, damping: 30 },
      }}
    >
      <m.span
        className="shrink-0"
        style={{ color }}
        initial={{ opacity: 0, y: 3 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: 0.12 }}
      >
        {ICON[kind]}
      </m.span>
      <m.span
        className="whitespace-nowrap text-xs text-fg"
        initial={{ opacity: 0, y: 3 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1], delay: 0.2 }}
      >
        {text}
      </m.span>
    </m.div>
  );
}
