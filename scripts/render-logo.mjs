// Pipeline de branding do Ember.
//
// A marca deixou de ser um SVG plano gerado por codigo: passou a ser uma imagem raster
// (estrela incandescente com grao->glow, a metafora do refine) desenhada externamente. Este
// script deixa de gerar o SVG antigo; documenta o fluxo atual para os icones nunca divergirem.
//
// Fluxo (a partir de uma imagem-fonte quadrada, idealmente >= 1024px, fundo transparente):
//
//   1. Preparar o master 1024 (recorta ao conteudo, quadra com margem, upscale Lanczos se
//      preciso, unsharp subtil). Feito com PIL:
//
//      python -c "from PIL import Image, ImageFilter; \
//        im=Image.open('fonte.png').convert('RGBA'); im=im.crop(im.getchannel('A').getbbox()); \
//        w,h=im.size; s=max(w,h); m=int(s*0.06); c=s+2*m; \
//        sq=Image.new('RGBA',(c,c),(0,0,0,0)); sq.paste(im,((c-w)//2,(c-h)//2),im); \
//        big=sq.resize((1024,1024),Image.LANCZOS).filter(ImageFilter.UnsharpMask(1.6,60,2)); \
//        big.save('logo-source-1024.png')"
//
//   2. Gerar todos os formatos (png/ico/icns/Square*/iOS/Android):
//
//      npx tauri icon logo-source-1024.png
//
// O header das Settings (src/components/Logo.tsx), o splash e o favicon do README usam os PNGs
// gerados em src-tauri/icons, por isso o passo 2 propaga a marca para todo o lado.

console.error(
  "render-logo.mjs ja nao gera a marca (deixou de ser SVG). Ve o comentario no topo: prepara o\n" +
    "master com PIL e corre `npx tauri icon logo-source-1024.png`."
);
process.exit(1);
