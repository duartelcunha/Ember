import { m } from "motion/react";

/**
 * A marca do Ember junto ao ponteiro enquanto um refine decorre.
 *
 * Historia deste ficheiro, porque ela explica a forma:
 *
 * 1. Era a estrela da marca, em raster, com brilho a mais: lia-se como um enfeite.
 * 2. Em 2026-08-30 a estrela foi retirada e ficaram tres pontinhos em orbita. Resolveu o brilho
 *    mas trocou identidade por movimento: o que aparecia junto ao rato podia ser de qualquer app.
 * 3. Agora: a estrela outra vez, mas VETORIAL e desenhada para 40px. O que a anima nao e uma
 *    rotacao (uma marca a girar le-se como um spinner de brinquedo) mas a propria historia do
 *    logo, que e a historia da app: o bruto a passar a polido. O gradiente varre a estrela do
 *    canto rugoso para a ponta acesa, em ciclo, enquanto o modelo trabalha.
 *
 * Regras que nao se negoceiam aqui:
 * - **Nada de `filter` animado.** Re-amostrar um blur a 120fps enquanto a janela persegue o rato
 *   engasga o overlay. O halo e estatico e pequeno.
 * - **Orcamento de brilho: 6px.** A janela do overlay so tem 8px logicos de padding (`p-2` no
 *   Overlay.tsx); tudo o que passe disso e cortado pela borda e deixa um edge duro.
 * - **Contraste sem brilho.** A legibilidade sobre um fundo claro vem de um contorno escuro
 *   (`paint-order: stroke`), nao de mais glow. Foi o que permitiu baixa-lo.
 */

/** Lado do quadrado da faisca, em px logicos. ESPELHADO em `DEFAULT_LAYOUT.spark`
 *  (ember-core/src/overlay_geom.rs); muda um, muda o outro, senao a estrela descentra-se do
 *  ponteiro e o clamp junto as bordas do ecra passa a estar errado. */
const SPARK_SIZE = 40;

/** A estrela de cinco pontas da marca, desenhada em coordenadas de 40x40 e inclinada 16 graus
 *  como no logo. Pontas longas e finas (raio interior a 37% do exterior): a 40px, uma estrela
 *  "gorda" perde a silhueta e passa a mancha. */
const STAR =
  "M24.96 2.7 L25.2 15.94 L37.99 19.37 L25.47 23.69 L26.16 36.91 " +
  "L18.18 26.34 L5.82 31.08 L13.4 20.23 L5.08 9.93 L17.74 13.8 Z";

/** Ritmo por estado. O retry acelera a varredura e cresce um pouco: e o unico sinal de que
 *  alguma coisa correu mal e esta a ser tentada outra vez, e tem de se notar sem alarmar. */
const VARIANT = {
  work: { scale: 1, sweep: 2.4 },
  retry: { scale: 1.24, sweep: 1.3 },
} as const;

export function Orb({ variant = "work" }: { variant?: keyof typeof VARIANT }) {
  const v = VARIANT[variant];
  return (
    <m.div
      className="relative shrink-0"
      style={{ width: SPARK_SIZE, height: SPARK_SIZE, willChange: "transform, opacity" }}
      initial={{ opacity: 0, scale: 0.4 }}
      animate={{ opacity: 1, scale: v.scale }}
      // Primeiro tempo do morph para a pilula: a estrela colapsa no ponteiro e a pilula acende
      // a partir desse ponto (ver Pill.tsx).
      exit={{ opacity: 0, scale: 0.15, transition: { duration: 0.13, ease: [0.4, 0, 1, 1] } }}
      transition={{
        opacity: { duration: 0.18, ease: "easeOut" },
        scale: { type: "spring", stiffness: 420, damping: 26 },
      }}
    >
      {/* Braseiro por tras da marca: um disco pequeno e ESTATICO. Da corpo a estrela sobre
          fundos escuros sem ser um halo; 32% cobria metade da caixa e lia-se como enfeite. */}
      <div
        aria-hidden
        className="pointer-events-none absolute"
        style={{
          inset: 8,
          borderRadius: 9999,
          background:
            "radial-gradient(circle, color-mix(in srgb, var(--color-accent) 18%, transparent) 0%, transparent 72%)",
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
          {/* A varredura bruto -> polido. `spreadMethod="reflect"` faz o ciclo fechar sem salto:
              a banda vai e volta em vez de reiniciar do zero. */}
          <linearGradient
            id="ember-sweep"
            x1="0"
            y1="40"
            x2="40"
            y2="0"
            gradientUnits="userSpaceOnUse"
            spreadMethod="reflect"
          >
            <stop offset="0%" stopColor="var(--color-ember-raw)" />
            <stop offset="45%" stopColor="var(--color-accent)" />
            <stop offset="100%" stopColor="var(--color-ember-glow)" />
            {/* SMIL e nao CSS: os stops de um gradiente SVG nao sao animaveis por CSS, e a
                alternativa (duas estrelas sobrepostas com uma mascara a mexer) custa uma camada
                composta a mais por frame enquanto a janela ja persegue o cursor a 120fps. */}
            <animateTransform
              attributeName="gradientTransform"
              type="translate"
              values="-26 26; 26 -26; -26 26"
              dur={`${v.sweep}s`}
              repeatCount="indefinite"
              calcMode="spline"
              keyTimes="0;0.5;1"
              keySplines="0.4 0 0.6 1; 0.4 0 0.6 1"
            />
          </linearGradient>
        </defs>
        {/* O contorno escuro por BAIXO do preenchimento (`paint-order`) e o que faz a marca
            existir sobre um fundo claro. Antes essa leitura vinha toda do brilho, e por isso o
            brilho nao podia descer. */}
        <path
          d={STAR}
          fill="url(#ember-sweep)"
          stroke="rgba(0,0,0,0.42)"
          strokeWidth="1.6"
          strokeLinejoin="round"
          style={{ paintOrder: "stroke" }}
        />
        {/* Fio interior claro: separa a marca do proprio contorno e da-lhe a aresta polida do
            logo, sem acrescentar area acesa. */}
        <path
          d={STAR}
          fill="none"
          stroke="color-mix(in srgb, var(--color-ember-glow) 55%, transparent)"
          strokeWidth="0.6"
          strokeLinejoin="round"
        />
      </svg>
    </m.div>
  );
}
