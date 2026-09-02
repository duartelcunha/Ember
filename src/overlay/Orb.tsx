import { m } from "motion/react";
import { useSyncExternalStore } from "react";

/**
 * O ponto que pisca ao lado do ponteiro enquanto um refine decorre.
 *
 * Historia deste ficheiro, porque cada tentativa falhou por uma razao diferente e vale a pena
 * nao repetir nenhuma:
 *
 * 1. A estrela do icone, em raster, com brilho a mais: lia-se como enfeite.
 * 2. Tres pontos em orbita: resolveu o brilho, mas o que aparecia junto ao rato podia ser de
 *    qualquer app.
 * 3. Uma estrela vetorial: recusada. A app chama-se Ember; uma estrela nao tem nada a ver.
 * 4. Uma brasa desenhada (carvao, crosta, fendas): identidade sim, mas literal de mais, e uma
 *    brasa a respirar diz "ha lume aceso", nao diz "estou a trabalhar".
 * 5. Uma costura de luz a percorrer uma linha: significava o que a app faz, mas nao brincava
 *    com o ponteiro, andava ao lado dele.
 * 6. Faiscas a sair da ponta, um anel a perseguir o cursor, a ponta incandescente: todas
 *    recusadas por serem barulhentas de mais para o sitio onde vivem.
 *
 * O que ficou e o oposto de tudo isso: um circulo pequeno ao lado do ponteiro, a piscar devagar.
 * Nao tenta significar nada, nao rouba atencao ao texto por baixo, e junto ao rato basta para
 * dizer que ha trabalho a decorrer. A identidade vem da COR, nao da forma: o piscar e uma
 * passagem entre os dois tons quentes da marca (`--color-accent` e `--color-ember-glow`), e nao
 * um fade para o nada.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. Os dois halos sao estaticos e curtos.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **Contraste sem brilho.** Sobre um fundo claro o ponto le-se por um `drop-shadow` ESCURO,
 *   nao por mais glow.
 * - **A saida nao se toca.** O `exit` daqui e o primeiro tempo de um morph: a pilula acende a
 *   partir do sitio onde isto colapsa (ver Pill.tsx). Mudar-lhe a duracao ou a escala parte a
 *   entrada dela.
 */

/** Lado do quadrado da faisca, em px logicos. ESPELHADO em `DEFAULT_LAYOUT.spark`
 *  (ember-core/src/overlay_geom.rs); muda um, muda o outro, senao o ponto descentra-se do
 *  ponteiro e o clamp junto as bordas do ecra passa a estar errado. */
const SPARK_SIZE = 40;

/** Onde o ponto vive dentro da caixa: AO LADO da seta, nunca por baixo dela.
 *
 *  Com a ancora atual (`pointer_center: (6, 9)` no overlay_geom.rs), o hotspot da seta cai em
 *  (14, 11) destas coordenadas e o corpo dela desce dali ate (26, 30), alargando a medida que
 *  desce. A (27, 13) o ponto fica a direita da seta, na parte onde ela ainda e estreita: ~9px de
 *  folga limpa. Mais para baixo ficaria por tras dela, que o sistema desenha por cima de nos. */
const DOT = { cx: 27, cy: 13, r: 3 };

/** Ritmo por estado. O retry pisca mais depressa e cresce um pouco: e o unico sinal de que
 *  alguma coisa correu mal e esta a ser tentada outra vez, e tem de se notar sem alarmar.
 *
 *  2.2s e deliberadamente lento. O spinner `embers` da casa (components/ui/spinner.tsx) corre a
 *  1.1s porque vive dentro de uma janela onde se esta a olhar para ele; este vive por cima do
 *  trabalho de outra pessoa e tem de ser mais quieto do que isso. */
const VARIANT = {
  work: { scale: 1, blink: 2.2 },
  retry: { scale: 1.15, blink: 1.1 },
} as const;

/** O piscar nunca chega ao zero. Um ponto que desaparece por completo le-se como avaria ou como
 *  "acabou"; parar a meio caminho le-se como respiracao. */
const DIM = 0.32;

/** O `MotionConfig reducedMotion="user"` do Overlay.tsx cobre o motion, mas NAO o SMIL do SVG:
 *  um `<animate>` continua a correr por baixo dele. Quem pediu ao sistema para parar as
 *  animacoes ficava com a unica coisa animada do ecra a piscar na mesma. */
function usePrefersReducedMotion() {
  return useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    },
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    () => false,
  );
}

export function Orb({ variant = "work" }: { variant?: keyof typeof VARIANT }) {
  const v = VARIANT[variant];
  const still = usePrefersReducedMotion();
  const breath = {
    dur: `${v.blink}s`,
    repeatCount: "indefinite" as const,
    calcMode: "spline" as const,
    keyTimes: "0;0.5;1",
    // Curva simetrica: acende e apaga ao mesmo ritmo, sem o solavanco de um ease-out.
    keySplines: "0.4 0 0.6 1; 0.4 0 0.6 1",
  };

  return (
    <m.div
      className="relative shrink-0"
      style={{
        width: SPARK_SIZE,
        height: SPARK_SIZE,
        willChange: "transform, opacity",
        // O colapso da saida acontece onde o ponto esta de facto, e nao no centro geometrico da
        // caixa, onde nao ha nada desenhado. 27/40 e 13/40 das coordenadas do DOT.
        transformOrigin: "67.5% 32.5%",
      }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: v.scale }}
      // Primeiro tempo do morph para a pilula: o ponto colapsa e a pilula acende a partir dai
      // (ver Pill.tsx). Duracao, curva e escala sao contratuais; nao lhes mexer.
      exit={{ opacity: 0, scale: 0.15, transition: { duration: 0.13, ease: [0.4, 0, 1, 1] } }}
      transition={{
        opacity: { duration: 0.18, ease: "easeOut" },
        scale: { type: "spring", stiffness: 420, damping: 26 },
      }}
    >
      <svg
        width={SPARK_SIZE}
        height={SPARK_SIZE}
        viewBox="0 0 40 40"
        fill="none"
        aria-hidden
        style={{
          // Dois halos ESTATICOS: o quente da-lhe presenca sobre fundos escuros, o escuro da-lhe
          // aresta sobre fundos claros, que e o que dispensa desenhar um contorno em volta.
          // Somados, 4.5px: dentro dos 6 do orcamento.
          filter:
            "drop-shadow(0 0 3px color-mix(in srgb, var(--color-accent) 45%, transparent)) drop-shadow(0 0 1.5px rgba(0,0,0,0.55))",
        }}
      >
        {/* Um sopro de calor em volta, estatico e pequeno. Sem ele o ponto le-se como um LED
            colado ao ecra; com ele parece aceso. Fica FORA do grupo que pisca, para o calor nao
            desaparecer junto com o ponto e o sitio continuar marcado. */}
        <circle cx={DOT.cx} cy={DOT.cy} r={DOT.r * 2.4} fill="var(--color-accent)" opacity="0.1" />

        {/* O ponto pisca como um todo: uma so animacao, de opacidade.
            As duas cores da marca vem da COMPOSICAO (corpo em accent, centro quente em glow) e
            nao de uma animacao de `fill`: o SMIL nao resolve variaveis CSS em valores de cor, e
            uma animacao dessas falhava em silencio, deixando o ponto de uma cor so. */}
        <g opacity={1}>
          {!still && <animate attributeName="opacity" values={`1;${DIM};1`} {...breath} />}
          <circle cx={DOT.cx} cy={DOT.cy} r={DOT.r} fill="var(--color-accent)" />
          <circle cx={DOT.cx} cy={DOT.cy} r={DOT.r * 0.45} fill="var(--color-ember-glow)" />
        </g>
      </svg>
    </m.div>
  );
}
