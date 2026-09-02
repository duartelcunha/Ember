import { m } from "motion/react";

/**
 * A brasa junto ao ponteiro enquanto um refine decorre.
 *
 * Historia deste ficheiro, porque ela explica a forma:
 *
 * 1. Era a estrela do icone, em raster, com brilho a mais: lia-se como um enfeite.
 * 2. Em 2026-08-30 a estrela foi retirada e ficaram tres pontinhos em orbita. Resolveu o brilho
 *    mas trocou identidade por movimento: o que aparecia junto ao rato podia ser de qualquer app.
 * 3. A estrela vetorial que se tentou a seguir foi recusada, e com razao: uma estrela nao tem
 *    nada a ver com o nome. **A app chama-se Ember.** O que aparece junto ao rato e uma BRASA:
 *    um carvao irregular com crosta escura na borda e nucleo incandescente, atravessado por
 *    fendas onde o calor se ve. Os proprios tokens da marca ja diziam isto e ninguem os tinha
 *    ouvido: `--color-ember-raw` (bruto), `--color-accent`, `--color-ember-glow` (incandescente).
 *
 * O movimento e a respiracao do calor: o nucleo cresce e recolhe, como uma brasa a arder. Nao
 * roda. Uma marca a girar le-se como um spinner de brinquedo; uma brasa a respirar diz que ha
 * trabalho a decorrer e e a mesma coisa que a app faz ao texto.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. O halo e estatico e pequeno.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **Contraste sem brilho.** Sobre um fundo claro a brasa le-se pela crosta escura
 *   (`paint-order: stroke`), nao por mais glow. Foi o que permitiu baixa-lo para metade.
 */

/** Lado do quadrado da faisca, em px logicos. ESPELHADO em `DEFAULT_LAYOUT.spark`
 *  (ember-core/src/overlay_geom.rs); muda um, muda o outro, senao a brasa descentra-se do
 *  ponteiro e o clamp junto as bordas do ecra passa a estar errado. */
const SPARK_SIZE = 40;

/** O carvao: contorno fechado e suave, ~26px de largura numa caixa de 40, deliberadamente
 *  irregular (mais largo que alto, com um lado achatado e um canto saliente). Um circulo aqui
 *  lia-se como um ponto de carregamento qualquer; o que faz isto ser uma brasa e a silhueta
 *  torta. Gerado a partir de nove raios por angulo, suavizado em bezier. */
const COAL =
  "M33.07 18.76 C33.38 20.77,30.94 24.68,29.67 26.64 C28.39 28.6,25.86 32.24,23.96 32.77 " +
  "C22.05 33.3,18.26 31.24,16.03 30.43 C13.79 29.62,9.11 28.72,7.99 26.98 " +
  "C6.88 25.25,7.26 20.08,8.07 18.06 C8.87 16.04,11.99 13.97,13.72 12.56 " +
  "C15.45 11.15,18.51 8.05,20.44 8.01 C22.37 7.97,25.73 10.77,27.49 12.28 " +
  "C29.26 13.78,32.77 16.75,33.07 18.76 Z";

/** As fendas por onde o calor sai: uma principal e um ramo, como um carvao que estalou. Sao o
 *  detalhe que faz a forma ler-se como materia em brasa e nao como uma bolha cor de laranja. */
const FISSURE_MAIN = "M11.9 16.4 C15.9 19.2,19.2 21.1,26.3 25.4";
const FISSURE_BRANCH = "M19.4 20.4 C20.9 17.6,22.4 15.1,23.9 12.7";

/** Ritmo por estado. O retry respira mais depressa e cresce um pouco: e o unico sinal de que
 *  alguma coisa correu mal e esta a ser tentada outra vez, e tem de se notar sem alarmar. */
const VARIANT = {
  work: { scale: 1, breath: 2.6 },
  retry: { scale: 1.24, breath: 1.3 },
} as const;

export function Orb({ variant = "work" }: { variant?: keyof typeof VARIANT }) {
  const v = VARIANT[variant];
  return (
    <m.div
      className="relative shrink-0"
      style={{ width: SPARK_SIZE, height: SPARK_SIZE, willChange: "transform, opacity" }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: v.scale }}
      // Primeiro tempo do morph para a pilula: a brasa colapsa no ponteiro e a pilula acende a
      // partir desse ponto (ver Pill.tsx).
      exit={{ opacity: 0, scale: 0.15, transition: { duration: 0.13, ease: [0.4, 0, 1, 1] } }}
      transition={{
        opacity: { duration: 0.18, ease: "easeOut" },
        scale: { type: "spring", stiffness: 420, damping: 26 },
      }}
    >
      {/* O calor que a brasa lanca a volta: disco ESTATICO e curto. Da-lhe corpo sobre fundos
          escuros sem virar halo; a 32% cobria metade da caixa e lia-se como enfeite. */}
      <div
        aria-hidden
        className="pointer-events-none absolute"
        style={{
          inset: 6,
          borderRadius: 9999,
          background:
            "radial-gradient(circle, color-mix(in srgb, var(--color-accent) 20%, transparent) 0%, transparent 70%)",
        }}
      />
      <svg
        width={SPARK_SIZE}
        height={SPARK_SIZE}
        viewBox="0 0 40 40"
        fill="none"
        aria-hidden
        className="relative"
        style={{
          // Halo estatico e curto: dentro do orcamento de 6px que a janela permite.
          filter: "drop-shadow(0 0 3px color-mix(in srgb, var(--color-accent) 45%, transparent))",
        }}
      >
        <defs>
          {/* O calor: nucleo incandescente descentrado (uma brasa nao arde por igual), a passar a
              cor de fogo e a acabar em bruto na borda, que e a crosta. */}
          <radialGradient id="ember-heat" cx="0.42" cy="0.36" r="0.62">
            <stop offset="0%" stopColor="var(--color-ember-glow)" />
            <stop offset="42%" stopColor="var(--color-accent)" />
            <stop offset="100%" stopColor="var(--color-ember-raw)" />
            {/* A respiracao. SMIL e nao CSS porque os stops e o raio de um gradiente SVG nao sao
                animaveis por CSS, e a alternativa (duas formas sobrepostas com opacidade a
                alternar) custa uma camada composta a mais por frame enquanto a janela ja
                persegue o cursor a 120fps. */}
            <animate
              attributeName="r"
              values="0.5;0.72;0.5"
              dur={`${v.breath}s`}
              repeatCount="indefinite"
              calcMode="spline"
              keyTimes="0;0.5;1"
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
            />
          </radialGradient>
        </defs>
        {/* A crosta escura fica por BAIXO do preenchimento (`paint-order`) e e o que faz a brasa
            existir sobre um fundo claro. Antes essa leitura vinha toda do brilho, e por isso o
            brilho nao podia descer. */}
        <path
          d={COAL}
          fill="url(#ember-heat)"
          stroke="rgba(0,0,0,0.45)"
          strokeWidth="1.6"
          strokeLinejoin="round"
          style={{ paintOrder: "stroke" }}
        />
        {/* As fendas acendem em contratempo com a respiracao: quando o nucleo recolhe, sao elas
            que continuam a mostrar que ha lume la dentro. */}
        <g
          stroke="var(--color-ember-glow)"
          strokeLinecap="round"
          fill="none"
          style={{ mixBlendMode: "screen" }}
        >
          <path d={FISSURE_MAIN} strokeWidth="1" opacity="0.9">
            <animate
              attributeName="opacity"
              values="0.9;0.45;0.9"
              dur={`${v.breath}s`}
              repeatCount="indefinite"
              calcMode="spline"
              keyTimes="0;0.5;1"
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
            />
          </path>
          <path d={FISSURE_BRANCH} strokeWidth="0.7" opacity="0.6">
            <animate
              attributeName="opacity"
              values="0.6;0.28;0.6"
              dur={`${v.breath}s`}
              repeatCount="indefinite"
              calcMode="spline"
              keyTimes="0;0.5;1"
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
            />
          </path>
        </g>
      </svg>
    </m.div>
  );
}
