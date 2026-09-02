import { m } from "motion/react";
import { useSyncExternalStore } from "react";

/**
 * O losango de pixels que corre ao lado do ponteiro enquanto um refine decorre.
 *
 * A coreografia vem do componente `diamond` do loading-ui.com (registo publico de componentes,
 * feito para ser copiado para dentro do projeto): oito quadrados dispostos num losango, cada um a
 * acender de golpe e a esvanecer, desfasados de 0.1s, num ciclo de 0.8s. O que resulta e um
 * cometa a dar a volta ao losango, com rasto.
 *
 * O que e nosso: a cor da marca em vez de `currentColor`, a grelha ajustada para a caixa da
 * overlay, a posicao ao lado da seta, o calor que abre com a espera, e o morph para a pilula.
 *
 * Historia deste ficheiro, porque cada tentativa falhou por uma razao diferente:
 *
 * 1. A estrela do icone, em raster, com brilho a mais: lia-se como enfeite.
 * 2. Tres pontos em orbita: sem identidade, podia ser de qualquer app.
 * 3. Uma estrela vetorial: uma estrela nao tem nada a ver com o nome da app.
 * 4. Uma brasa desenhada (carvao, crosta, fendas): literal de mais.
 * 5. Uma costura de luz numa linha: significava o que a app faz, mas nao brincava com o ponteiro.
 * 6. Faiscas da ponta, um anel a perseguir, a ponta acesa: barulhentas de mais.
 * 7. Um ponto a piscar: piscar e vocabulario de alerta e puxava o olho de quem tentava ler.
 * 8. Um ponto a respirar: bem, mas quieto de mais para dizer que ha trabalho a decorrer.
 *
 * Nota de registo: o pixel-art e um registo diferente do resto da app (Geist, vidro, gradientes
 * quentes). Fica coerente por duas coisas: os pixels usam o acento do projeto como tudo o resto,
 * e o retro nao contamina mais nada. Se um dia parecer barulhento por cima de texto, o unico
 * numero a mexer e o `chase` do `VARIANT`.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. O calor e um gradiente SVG, nao um blur.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **A saida nao se toca.** O `exit` daqui e o primeiro tempo de um morph: a pilula acende a
 *   partir do sitio onde isto colapsa (ver Pill.tsx). Mudar-lhe a duracao ou a escala parte a
 *   entrada dela.
 */

/** Lado do quadrado da faisca, em px logicos. ESPELHADO em `DEFAULT_LAYOUT.spark`
 *  (ember-core/src/overlay_geom.rs); muda um, muda o outro, senao o losango descentra-se do
 *  ponteiro e o clamp junto as bordas do ecra passa a estar errado. */
const SPARK_SIZE = 40;

/** Lado de cada pixel. Tres, e nao um numero qualquer: o losango tem cinco celulas de lado, e
 *  3x5 = 15 poe todos os cantos em coordenadas INTEIRAS da caixa. Num motivo de pixel-art, meia
 *  celula fora da grelha e o unico defeito que se ve de imediato. */
const PX = 3;

/** Canto superior-esquerdo da grelha de 5x5 celulas, AO LADO da seta.
 *
 *  Com a ancora atual (`pointer_center: (6, 9)` no overlay_geom.rs), o hotspot da seta cai em
 *  (14, 11) destas coordenadas e o corpo dela desce dali ate (26, 30), alargando a medida que
 *  desce. Com a grelha em (22, 2), o pixel mais proximo (o de baixo-esquerda, em 25,11) fica a
 *  8px da aresta da seta. Mais para dentro ou mais para baixo e o sistema desenha o cursor por
 *  cima do losango. */
const GRID = { x: 22, y: 2 };

/** As oito posicoes, em CELULAS, na ordem em que acendem: cima, e dai para a direita ate voltar.
 *  E a ordem que faz o cometa dar a volta em vez de piscar em desordem. */
const RING = [
  [2, 0], // cima
  [3, 1], // cima-direita
  [4, 2], // direita
  [3, 3], // baixo-direita
  [2, 4], // baixo
  [1, 3], // baixo-esquerda
  [0, 2], // esquerda
  [1, 1], // cima-esquerda
] as const;

/** Centro do losango, para o calor e para o ponto de colapso da saida. */
const CENTER = { x: GRID.x + PX * 2.5, y: GRID.y + PX * 2.5 };

/** O calor em volta, ao longo da espera. Raio em px da caixa.
 *
 *  A forma da curva e o que faz isto significar alguma coisa: quase nao mexe nos primeiros
 *  segundos e so abre a serio quando a espera sai do normal (~5s nesta maquina, medido no log).
 *  Nos casos correntes ninguem chega a ver isto crescer, e e esse o objectivo: um indicador que
 *  fala sempre nao esta a dizer nada. A pergunta a que responde nao e "esta a trabalhar?", que
 *  quem carregou no atalho ja sabe, e "esta preso?".
 *
 *  O teto e geometrico: o `viewBox` corta o que sai dele, e com o centro em (29.5, 9.5) um raio
 *  de 15 so deixa de fora a franja do gradiente, que ai ja e transparente. */
const HEAT = {
  values: "10;10.6;12.5;15",
  times: "0;0.2;0.5;1",
  splines: "0.4 0 0.6 1; 0.4 0 0.6 1; 0.4 0 0.6 1",
  over: 30,
};

/** Ritmo por estado. O retry corre mais depressa e cresce um pouco: e o unico sinal de que
 *  alguma coisa correu mal e esta a ser tentada outra vez. Os 0.8s do modo normal sao os do
 *  componente original; o desfasamento e sempre um oitavo do ciclo, que e o que mantem o rasto
 *  distribuido a volta. */
const VARIANT = {
  work: { scale: 1, chase: 0.8 },
  retry: { scale: 1.15, chase: 0.5 },
} as const;

/** O `MotionConfig reducedMotion="user"` do Overlay.tsx cobre o motion, mas NAO o SMIL do SVG.
 *  As keyframes CSS tratam-se sozinhas com uma media query (ver `css` abaixo); esta hook e so
 *  para o calor, que e SMIL e nao a obedece. */
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

/** Keyframes injetadas no proprio SVG, como no `Spinner` da casa (components/ui/spinner.tsx):
 *  a peca fica auto-contida em vez de espalhar um `@keyframes` de uso unico pelo globals.css.
 *
 *  Acende de golpe (1% do ciclo) e esvanece o resto do tempo: e isso que da o rasto de cometa, e
 *  nao uma sequencia de piscas. Com movimento reduzido, cada pixel fica na opacidade base que
 *  leva no atributo, o que congela o cometa em vez de o apagar. */
function css(chase: number) {
  const steps = RING.map(
    (_, i) => `.ember-px-${i}{animation:ember-chase ${chase}s ease-in-out ${(
      (chase / RING.length) *
      i
    ).toFixed(3)}s infinite}`,
  ).join("");
  return `@keyframes ember-chase{0%{opacity:0}1%{opacity:1}100%{opacity:0}}${steps}
@media (prefers-reduced-motion: reduce){[class^="ember-px-"]{animation:none}}`;
}

export function Orb({ variant = "work" }: { variant?: keyof typeof VARIANT }) {
  const v = VARIANT[variant];
  const still = usePrefersReducedMotion();

  return (
    <m.div
      className="relative shrink-0"
      style={{
        width: SPARK_SIZE,
        height: SPARK_SIZE,
        willChange: "transform, opacity",
        // O colapso da saida acontece no centro do losango, e nao no centro geometrico da caixa,
        // onde nao ha nada desenhado.
        transformOrigin: `${(CENTER.x / SPARK_SIZE) * 100}% ${(CENTER.y / SPARK_SIZE) * 100}%`,
      }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: v.scale }}
      // Primeiro tempo do morph para a pilula: o losango colapsa e a pilula acende a partir dai
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
          // UMA sombra, escura e curta, so para os pixels terem aresta sobre fundos claros.
          filter: "drop-shadow(0 0 1.5px rgba(0,0,0,0.55))",
        }}
      >
        <style>{css(v.chase)}</style>
        <defs>
          {/* O calor: desvanece ate zero na borda, portanto nao tem contorno nenhum. Fica discreto
              de proposito, para nao competir com a aresta dura dos pixels, que e o que da o
              caracter a esta peca. */}
          <radialGradient id="ember-heat">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.26" />
            <stop offset="45%" stopColor="var(--color-accent)" stopOpacity="0.1" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0" />
          </radialGradient>
        </defs>

        <circle cx={CENTER.x} cy={CENTER.y} r={10} fill="url(#ember-heat)">
          {!still && (
            <animate
              attributeName="r"
              values={HEAT.values}
              keyTimes={HEAT.times}
              keySplines={HEAT.splines}
              dur={`${HEAT.over}s`}
              begin="0s"
              repeatCount="1"
              fill="freeze"
              calcMode="spline"
            />
          )}
        </circle>

        {RING.map(([col, row], i) => (
          <rect
            key={i}
            className={`ember-px-${i}`}
            x={GRID.x + col * PX}
            y={GRID.y + row * PX}
            width={PX}
            height={PX}
            fill="var(--color-accent)"
            // Sem suavizacao de arestas: num motivo de pixel-art, um quadrado esborratado nas
            // bordas deixa de ser um pixel. E tambem o que o mantem nitido a 125% e 150%.
            shapeRendering="crispEdges"
            // Opacidade base, que so se ve com movimento reduzido: congela o cometa a meio da
            // volta em vez de o apagar.
            opacity={1 - i * 0.11}
          />
        ))}
      </svg>
    </m.div>
  );
}
