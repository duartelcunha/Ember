// Renderiza o mark "Tile + spark" para PNG 1024 (fonte para `tauri icon`).
// Flat, uma so cor de marca; o spark e negative space (furo) dentro do tile.
import { Resvg } from "@resvg/resvg-js";
import { writeFileSync } from "node:fs";

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 64 64">
  <path fill="#ff7a18" fill-rule="evenodd" d="M20 6 h24 a14 14 0 0 1 14 14 v24 a14 14 0 0 1 -14 14 h-24 a14 14 0 0 1 -14 -14 v-24 a14 14 0 0 1 14 -14 Z
    M32 17 C 33 28 36 31 47 32 C 36 33 33 36 32 47 C 31 36 28 33 17 32 C 28 31 31 28 32 17 Z"/>
</svg>`;

const png = new Resvg(svg, { fitTo: { mode: "width", value: 1024 } }).render().asPng();
writeFileSync("logo-1024.png", png);
console.log("wrote logo-1024.png");
