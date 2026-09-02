import { m } from "motion/react";
import { useSyncExternalStore } from "react";

/**
 * O ponto que respira ao lado do ponteiro enquanto um refine decorre.
 *
 * Historia deste ficheiro, porque cada tentativa falhou por uma razao diferente e vale a pena
 * nao repetir nenhuma:
 *
 * 1. A estrela do icone, em raster, com brilho a mais: lia-se como enfeite.
 * 2. Tres pontos em orbita: resolveu o brilho, mas podia ser de qualquer app.
 * 3. Uma estrela vetorial: recusada. A app chama-se Ember; uma estrela nao tem nada a ver.
 * 4. Uma brasa desenhada (carvao, crosta, fendas): literal de mais. Dizia "ha lume aceso", nao
 *    dizia "estou a trabalhar".
 * 5. Uma costura de luz a percorrer uma linha: significava o que a app faz, mas andava ao lado
 *    do ponteiro em vez de brincar com ele.
 * 6. Faiscas da ponta, um anel a perseguir, a ponta acesa: barulhentas de mais para o sitio.
 * 7. Um ponto a PISCAR: quase la, mas piscar e vocabulario de alerta (gravacao, notificacao,
 *    erro) e o olho esta cablado para detetar intermitencia. Durante uma espera longa, aquilo
 *    puxava a atencao de quem estava a tentar ler.
 *
 * O que ficou: um ponto pequeno ao lado do ponteiro que RESPIRA (amplitude curta, ciclo lento:
 * sente-se de canto de olho, nao se ve), e cujo calor em volta CRESCE com a espera.
 *
 * O crescimento e a unica coisa aqui que acrescenta informacao em vez de decorar. A pergunta que
 * alguem tem a olhar para isto nao e "esta a trabalhar?" (ele carregou no atalho, ja sabe), e
 * "esta preso?". Um indicador de ritmo constante e igual aos dois segundos e aos quarenta; com o
 * calor a abrir devagar, uma espera longa PARECE longa, e le-se o tempo sem haver numero nenhum.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. O crescimento e um gradiente SVG, nao um blur.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **Contraste sem brilho.** Sobre fundo claro o ponto le-se por UMA sombra escura e curta. O
 *   halo quente que existia por cima disso somava-se ao calor e dava um brilho sujo.
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

/** O calor em volta, ao longo da espera. Raio em px da caixa, tempo em fraccao de `over`.
 *
 *  A forma da curva e o que faz isto significar alguma coisa, e a primeira tentativa estava ao
 *  contrario: com um ease-out, aos 2s ja ia em 63% do percurso. Um refine normal demora ~5s
 *  (medido no log), portanto o sinal disparava SEMPRE e deixava de distinguir o que quer que
 *  fosse. Agora quase nao mexe nos primeiros segundos e so abre a serio quando a espera sai do
 *  normal: nos casos correntes ninguem chega a ver isto crescer, que e o objectivo. Um indicador
 *  que fala sempre nao esta a dizer nada.
 *
 *  O teto de 12 nao e estetico, e geometrico: o `viewBox` corta o que sai dele, e com o centro
 *  em (27, 13) um raio de 12 chega a x=39 e a y=1, o maior circulo que ainda cabe. */
const HEAT = {
  values: "6;6.6;9;12",
  times: "0;0.2;0.5;1",
  splines: "0.4 0 0.6 1; 0.4 0 0.6 1; 0.4 0 0.6 1",
  over: 30,
};

/** Ritmo por estado. O retry respira mais depressa e cresce um pouco: e o unico sinal de que
 *  alguma coisa correu mal e esta a ser tentada outra vez.
 *
 *  2.8s e deliberadamente lento. O spinner `embers` da casa (components/ui/spinner.tsx) corre a
 *  1.1s porque vive dentro de uma janela onde se esta a olhar para ele; este vive por cima do
 *  trabalho de outra pessoa e tem de ser mais quieto do que isso. */
const VARIANT = {
  work: { scale: 1, breath: 2.8 },
  retry: { scale: 1.15, breath: 1.4 },
} as const;

/** Fundo da respiracao. Era 0.32 quando isto piscava, e 0.32 e amplitude de alerta: o olho
 *  deteta a intermitencia sozinho e vai la ter, mesmo quando esta a ler outra coisa. A 0.7
 *  sente-se que ha vida sem se ser chamado. */
const REST = 0.7;

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
          // UMA sombra, escura e curta, so para o ponto ter aresta sobre fundos claros. O brilho
          // quente que aqui estava somava-se ao calor desenhado e dava um halo sujo, com dois
          // gradientes a competir; o calor faz esse trabalho melhor e sem borda.
          filter: "drop-shadow(0 0 1.5px rgba(0,0,0,0.55))",
        }}
      >
        <defs>
          {/* O ponto: uma conta de vidro quente, iluminada de cima-esquerda. As tres paragens da
              marca dao-lhe volume (incandescente no realce, fogo no corpo, bruto na aresta em
              sombra) em vez de o pintarem de uma cor chapada. */}
          <radialGradient id="ember-dot" cx="0.35" cy="0.3" r="0.85">
            <stop offset="0%" stopColor="var(--color-ember-glow)" />
            <stop offset="45%" stopColor="var(--color-accent)" />
            <stop offset="100%" stopColor="var(--color-ember-raw)" />
          </radialGradient>

          {/* O calor: desvanece ATE ZERO na borda, por isso nao tem contorno nenhum. Um disco
              chapado a 10% tinha uma aresta visivel, que e o que fazia o halo parecer colado. */}
          <radialGradient id="ember-heat">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.34" />
            <stop offset="45%" stopColor="var(--color-accent)" stopOpacity="0.13" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* O calor abre devagar com a espera, uma vez so e sem voltar atras (`fill="freeze"`).
            A duracao NAO vem do `VARIANT` de proposito: quando o refine passa a retry, o
            componente re-renderiza, e mudar o `dur` de uma animacao a meio reiniciava-a. O tempo
            decorrido nao pode ser reposto a zero precisamente quando a espera se alonga. */}
        <circle cx={DOT.cx} cy={DOT.cy} r={6} fill="url(#ember-heat)">
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

        {/* A respiracao: uma animacao so, de opacidade, e de amplitude curta. */}
        <circle cx={DOT.cx} cy={DOT.cy} r={DOT.r} fill="url(#ember-dot)" opacity={1}>
          {!still && (
            <animate
              attributeName="opacity"
              values={`1;${REST};1`}
              dur={`${v.breath}s`}
              repeatCount="indefinite"
              calcMode="spline"
              keyTimes="0;0.5;1"
              // Curva simetrica: enche e esvazia ao mesmo ritmo, como respirar.
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
            />
          )}
        </circle>
      </svg>
    </m.div>
  );
}
