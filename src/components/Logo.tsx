import logoUrl from "../../src-tauri/icons/128x128@2x.png";

/** A marca Ember: estrela incandescente que se afina de bruto (granulado) a polido (glow),
 *  a propria metafora do refine. Imagem raster (grao + glow, nao vetorizavel); a mesma fonte
 *  dos icones da app, para a marca ser consistente em todo o lado. */
export function Logo({ size = 32 }: { size?: number }) {
  return (
    <img
      src={logoUrl}
      width={size}
      height={size}
      alt="Ember"
      draggable={false}
      className="select-none"
      style={{ display: "block" }}
    />
  );
}
