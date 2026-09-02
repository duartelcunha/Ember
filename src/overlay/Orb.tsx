import { m } from "motion/react";
import { useSyncExternalStore } from "react";

/**
 * A costura de luz junto ao ponteiro enquanto um refine decorre.
 *
 * Historia deste ficheiro, porque ela explica a forma:
 *
 * 1. Era a estrela do icone, em raster, com brilho a mais: lia-se como um enfeite.
 * 2. Em 2026-08-30 a estrela foi retirada e ficaram tres pontinhos em orbita. Resolveu o brilho
 *    mas trocou identidade por movimento: o que aparecia junto ao rato podia ser de qualquer app.
 * 3. Uma estrela vetorial: recusada, e com razao. A app chama-se Ember; uma estrela nao tem nada
 *    a ver com isso.
 * 4. Uma brasa desenhada (carvao com crosta, nucleo e fendas): tinha identidade e respeitava o
 *    nome, mas era LITERAL de mais e nao lia como loading. Uma brasa a respirar diz "ha lume
 *    aceso", nao diz "estou a trabalhar neste texto".
 *
 * O que ficou: uma linha por onde viaja um ponto de luz, deixando ordem atras de si. A frente do
 * ponto a linha ondula (por refinar), atras fica direita (refinado). E o que a app faz ao texto,
 * em geometria pura, sem desenhar brasa nenhuma. O ponto de luz e a brasa, e e a unica coisa que
 * sobra da metafora.
 *
 * As tres paragens de cor da marca sao os tres estados da linha, cada uma a dizer o que o nome
 * dela diz: `--color-ember-raw` a frente (bruto), `--color-ember-glow` no ponto (incandescente),
 * e o rasto a arrefecer de `--color-accent` para o glow. Nao dependem de matiz: cada projeto
 * reata as tres (ver `projects.rs`) e a paleta Slate e cinzenta. O que faz isto ler-se e a
 * LUMINOSIDADE das tres, que se mantem nas oito paletas.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. Os dois halos sao estaticos e curtos.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **Contraste sem brilho.** Sobre um fundo claro a costura le-se por um `drop-shadow` ESCURO,
 *   nao por mais glow. Foi o que permitiu manter o halo quente pequeno.
 * - **A saida nao se toca.** O `exit` daqui e o primeiro tempo de um morph: a pilula acende a
 *   partir do sitio onde isto colapsa (ver Pill.tsx). Mudar-lhe a duracao ou a escala parte a
 *   entrada dela.
 */

/** Lado do quadrado da faisca, em px logicos. ESPELHADO em `DEFAULT_LAYOUT.spark`
 *  (ember-core/src/overlay_geom.rs); muda um, muda o outro, senao a costura descentra-se do
 *  ponteiro e o clamp junto as bordas do ecra passa a estar errado. */
const SPARK_SIZE = 40;

/** A costura vive no terco de CIMA da caixa, e nao no meio, por causa do cursor.
 *
 *  Com a ancora atual (`pointer_center: (6, 9)` no overlay_geom.rs), o hotspot da seta cai em
 *  (14, 11) das coordenadas desta caixa e o corpo dela desce dali ate (26, 30). Uma linha
 *  centrada em y=20 atravessava a seta e lia-se como um risco por cima dela. A y=5.5, com a onda
 *  a ocupar 4.0..7.0, sobram ~4px de folga sobre a ponta. */
const SEAM_Y = 5.5;
const SEAM_X0 = 8;
const SEAM_X1 = 38;

/** A linha ja resolvida: o rasto que o ponto deixa atras de si. */
const STRAIGHT = `M${SEAM_X0} ${SEAM_Y} L${SEAM_X1} ${SEAM_Y}`;

/** A linha por resolver: seno de amplitude 1.5, tres periodos, amostrado a seis pontos por
 *  periodo e suavizado em Catmull-Rom. Tres periodos e o minimo para se ler como ondulacao e nao
 *  como um traco torto; mais do que isso, a 40px, vira ruido. */
const WAVE =
  "M8 5.5 C8.28 5.72,9.11 6.58,9.67 6.8 C10.22 7.02,10.78 7.02,11.33 6.8 " +
  "C11.89 6.58,12.44 5.93,13 5.5 C13.56 5.07,14.11 4.42,14.67 4.2 " +
  "C15.22 3.98,15.78 3.98,16.33 4.2 C16.89 4.42,17.44 5.07,18 5.5 " +
  "C18.56 5.93,19.11 6.58,19.67 6.8 C20.22 7.02,20.78 7.02,21.33 6.8 " +
  "C21.89 6.58,22.44 5.93,23 5.5 C23.56 5.07,24.11 4.42,24.67 4.2 " +
  "C25.22 3.98,25.78 3.98,26.33 4.2 C26.89 4.42,27.44 5.07,28 5.5 " +
  "C28.56 5.93,29.11 6.58,29.67 6.8 C30.22 7.02,30.78 7.02,31.33 6.8 " +
  "C31.89 6.58,32.44 5.93,33 5.5 C33.56 5.07,34.11 4.42,34.67 4.2 " +
  "C35.22 3.98,35.78 3.98,36.33 4.2 C36.89 4.42,37.72 5.28,38 5.5";

/** Onde a costura esta quando o movimento esta desligado (`prefers-reduced-motion`): a meio-e-um
 *  pouco, que e a posicao onde os tres estados se veem todos. */
const STILL_AT = 0.65;

/** Ritmo por estado. O retry acelera e cresce 15%: e o unico sinal de que alguma coisa correu mal
 *  e esta a ser tentada outra vez, e tem de se notar sem alarmar.
 *
 *  Os tempos nao sao numeros novos: 1.6s fica entre o spinner `embers` da casa (1.1s, ver
 *  components/ui/spinner.tsx) e a respiracao que a brasa anterior tinha (2.6s); 1.0s do retry e
 *  o do spinner `arc`. O crescimento cabe com folga no `spark_clamp` (56px) do Rust. */
const VARIANT = {
  work: { scale: 1, pass: 1.6 },
  retry: { scale: 1.15, pass: 1.0 },
} as const;

/** Fracao do ciclo em que o ponto chega ao fim; o resto e o batimento que repoe a onda. */
const TRAVEL_END = 0.88;

/** O `MotionConfig reducedMotion="user"` do Overlay.tsx cobre o motion, mas NAO o SMIL do SVG:
 *  um `<animate>` continua a correr por baixo dele. Quem pediu ao sistema para parar as
 *  animacoes ficava com a unica coisa animada do ecra a mexer-se na mesma. */
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
  const dur = `${v.pass}s`;
  // A viagem ocupa `TRAVEL_END` do ciclo e depois fica quieta enquanto o batimento repoe a onda.
  const travel = { keyTimes: `0;${TRAVEL_END};1`, dur, repeatCount: "indefinite" as const };
  // Curva simetrica (acelera e trava), a mesma que os `<animate>` deste ficheiro ja usavam. O
  // segundo par mantem o valor final durante o batimento, sem deslizar.
  const ease = { calcMode: "spline" as const, keySplines: "0.4 0 0.6 1; 0 0 1 1" };

  return (
    <m.div
      className="relative shrink-0"
      style={{
        width: SPARK_SIZE,
        height: SPARK_SIZE,
        willChange: "transform, opacity",
        // O colapso da saida acontece onde a costura esta de facto (topo da caixa), e nao no
        // centro geometrico, onde ja nao ha nada desenhado. 14% de 40px = y 5.6, o SEAM_Y.
        transformOrigin: "50% 14%",
      }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: v.scale }}
      // Primeiro tempo do morph para a pilula: a costura colapsa e a pilula acende a partir desse
      // ponto (ver Pill.tsx). Duracao, curva e escala sao contratuais; nao lhes mexer.
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
          // aresta sobre fundos claros. O escuro e o que substitui o contorno por baixo de cada
          // traco, que custava duplicar cada elemento. Somados, 4.5px: dentro dos 6 do orcamento.
          filter:
            "drop-shadow(0 0 3px color-mix(in srgb, var(--color-accent) 45%, transparent)) drop-shadow(0 0 1.5px rgba(0,0,0,0.6))",
        }}
      >
        <defs>
          {/* O rasto arrefece da direita (acabado de passar, quente) para a esquerda (ja frio). */}
          <linearGradient
            id="ember-trail"
            x1={SEAM_X0}
            y1="0"
            x2={SEAM_X1}
            y2="0"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="var(--color-accent)" />
            <stop offset="100%" stopColor="var(--color-ember-glow)" />
          </linearGradient>
        </defs>

        {/* O batimento de reinicio. Sem ele, o ponto saltava do fim para o inicio e via-se o
            corte; assim, cada passagem le-se como mais uma linha de texto a ser tratada. */}
        <g>
          {!still && (
            <animate
              attributeName="opacity"
              values="1;1;0;1"
              keyTimes={`0;${TRAVEL_END};0.94;1`}
              dur={dur}
              repeatCount="indefinite"
              calcMode="spline"
              keySplines="0 0 1 1; 0.22 1 0.36 1; 0.22 1 0.36 1"
            />
          )}

          {/* A FRENTE do ponto: por refinar. Apaga-se pela esquerda a medida que a costura passa.
              Com `pathLength="1"` a matematica do tracejado e a mesma nas duas linhas, apesar de
              a onda ser mais comprida do que a recta. Um `clipPath` com rectangulos animados dava
              o mesmo resultado com mais markup e mais uma camada composta por frame, o que nao se
              quer numa janela que ja persegue o cursor a 120fps. */}
          <path
            d={WAVE}
            pathLength={1}
            stroke="var(--color-ember-raw)"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeDasharray="1 1"
            strokeDashoffset={still ? -STILL_AT : 0}
          >
            {!still && (
              <animate
                attributeName="stroke-dashoffset"
                values="0;-1;-1"
                {...travel}
                {...ease}
              />
            )}
          </path>

          {/* ATRAS do ponto: resolvido. Desenha-se da esquerda, no espelho exato do apagamento
              da onda. As duas coisas sao a mesma animacao vista dos dois lados, e e isso que faz
              a costura ler-se como UMA frente e nao como duas linhas independentes. */}
          <path
            d={STRAIGHT}
            pathLength={1}
            stroke="url(#ember-trail)"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeDasharray="1 1"
            strokeDashoffset={still ? 1 - STILL_AT : 1}
          >
            {!still && (
              <animate attributeName="stroke-dashoffset" values="1;0;0" {...travel} {...ease} />
            )}
          </path>

          {/* A costura em si: o unico ponto quente, e tudo o que resta da brasa. */}
          <circle
            cx={still ? SEAM_X0 + (SEAM_X1 - SEAM_X0) * STILL_AT : SEAM_X0}
            cy={SEAM_Y}
            r="1.8"
            fill="var(--color-ember-glow)"
          >
            {!still && (
              <animate
                attributeName="cx"
                values={`${SEAM_X0};${SEAM_X1};${SEAM_X1}`}
                {...travel}
                {...ease}
              />
            )}
          </circle>
        </g>
      </svg>
    </m.div>
  );
}
