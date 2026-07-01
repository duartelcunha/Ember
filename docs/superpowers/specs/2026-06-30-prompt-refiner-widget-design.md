# Ember (Prompt Refiner Widget), design

> Spec validado em sessao de brainstorming a 2026-06-30, endurecido por um pass de
> pesquisa + revisao adversarial (4 agentes: Tauri nativo, providers, UI premium,
> e stress-test do loop). Veredicto adversarial: PROCEED_WITH_CHANGES, com as
> mitigacoes nomeadas abaixo baked-in como requisitos de primeira classe.
>
> Nome do produto: **Ember** (o orb laranja a brilhar = uma brasa que se atica ao
> refinar; o orb ao vivo e o proprio logo). Projeto LOCAL: sem git/GitHub, sem remote.

## 1. Visao e veredicto

Widget de desktop para Windows que refina prompts no momento, em qualquer app. O
utilizador seleciona o texto em bruto num chat (Claude.ai, Gemini, ChatGPT, Word,
email), carrega numa hotkey global, um orb laranja aparece junto ao cursor, o texto
e refinado por um LLM guiado pelo perfil do utilizador, e ao aceitar o texto refinado
substitui a selecao no proprio sitio.

Faz sentido e e pratico. O que o distingue de um "melhorador" generico:
refina **in-place em qualquer app**, com **qualidade adaptativa**, guiado pelo
**perfil auto-detetado** do utilizador, **resiliente** por design (provider primario
+ fallback), e com **degradacao honesta** quando o alvo nao e suportado.

A parte dificil nao e a bolinha: e capturar texto de qualquer app e devolve-lo
corretamente sem partir foco, clipboard ou selecao. O spec trata isso como o nucleo.

## 2. Decisoes fechadas (brainstorming)

| Decisao | Escolha |
|---|---|
| Onde vive | Widget flutuante, hotkey global, orb junto ao cursor, sobre qualquer app |
| Interacao | Preview com aceitar instantaneo (Enter aceita e substitui, Esc cancela) |
| Refinamento | Adaptativo (escala a agressividade ao input, preserva a intencao) |
| Perfil | Auto-deteta CLAUDE.md + painel editavel no app + perfil "qualidade" por defeito |
| Chaves | BYOK (cada utilizador traz a sua chave Gemini; Claude opcional) |
| Stack | Tauri 2.x (nucleo Rust + frontend React/Tailwind) |
| UI | Premium: animacoes, logo, componentes de alta qualidade |

## 3. Arquitetura

Dois mundos com fronteira limpa.

**Nucleo Rust** (a parte sensivel, isolada e testavel):
- Hotkey global, captura/substituicao de texto, deteccao de alvos nao suportados.
- Chamadas aos providers com a maquina de retry/fallback (pura + orquestrador fino).
- Storage seguro das chaves (Windows Credential Manager).
- Auto-deteccao do ficheiro de perfil (CLAUDE.md).
- Todas as chamadas LLM e probes de validacao de chave saem do Rust, nunca do webview
  (as chaves nunca passam pela camada JS).

**Frontend React/Tailwind** (apresentacao):
- Janela overlay (o orb + a bolha de preview).
- Janela de settings (chaves, perfil, hotkey, aparencia, sobre).
- Icone na tray com menu.

Comunicacao via comandos Tauri (`#[tauri::command]`) e eventos. O frontend nunca ve
chaves nem texto persistido; recebe o texto refinado para mostrar e emite "aceitar".

### Janelas
1. **Overlay**: UMA janela transparente, sem decoracoes, always-on-top, skip-taskbar,
   nao-ativavel (nao rouba foco), dimensionada para os limites maximos da bolha
   (~420x320 px logicos). O orb e a bolha sao DOM dentro desta janela fixa; o
   crescimento e um morph CSS, NUNCA um resize da janela do SO (resizes do SO nao
   sao spring-driven e tremem).
2. **Settings**: janela normal, glassmorphism, tabs.
3. **Tray**: icone + menu (Abrir settings, Pausar, Sair).

## 4. O loop magico (sequencia endurecida)

Esta e a sequencia correta, ja com as mitigacoes adversariais. Cada passo no nucleo Rust.

1. **Hotkey disparada.** Guarda imediatamente `GetForegroundWindow()` -> `target_hwnd`
   (a janela onde o texto vive). Captura a posicao do cursor.
2. **Neutraliza modificadores presos.** O utilizador pode ainda estar a segurar
   Ctrl/Shift/Alt da propria hotkey. Espera (com timeout curto) ate `GetAsyncKeyState`
   mostrar Ctrl/Alt/Shift/Win libertados, ou injeta key-ups explicitos. So depois injeta.
3. **Deteta alvo nao suportado** (ver seccao 6). Se for app elevada (UIPI), terminal,
   fullscreen-exclusive ou secure desktop: aborta com mensagem honesta, nao continua.
4. **Captura a selecao deterministicamente.** Le `GetClipboardSequenceNumber()` ANTES.
   Injeta Ctrl+C (via virtual-key codes, nao Unicode, ver seccao 5). Faz poll do
   sequence number ate mudar, com hard timeout (300-800 ms). Exige `CF_UNICODETEXT`
   presente no novo conteudo.
   - **Se o sequence number NAO muda dentro do timeout** -> nada estava selecionado.
     NAO refina o clipboard anterior. Abre a bolha com a sua **caixa de input propria**
     para o utilizador escrever/colar ali (este e o caminho "nada selecionado").
5. **Snapshot do clipboard anterior** (best-effort, todos os formatos enumeraveis) para
   restaurar no fim. Guarda tambem o texto original capturado em memoria (para o "Revert").
6. **Mostra a overlay** junto ao cursor (DPI-correto, seccao 11), orb laranja a pulsar.
   A janela e nao-ativavel: o `target_hwnd` mantem o foco.
7. **Refina** (seccao 9). Streaming opcional para preview ao vivo. Em erro: retry/fallback
   ou degrada com estado visivel.
8. **Preview.** O orb faz morph para a bolha com o texto refinado. Teclas: Enter aceita,
   Esc cancela, R refina de novo, C copia.
9. **Aceitar.**
   a. **Re-valida o alvo.** Confirma que `target_hwnd` ainda existe e e o foreground.
      Se mudou (o utilizador clicou noutro lado durante a latencia do LLM), aborta com
      mensagem em vez de colar as cegas.
   b. **Restaura foco** ao `target_hwnd` via `AttachThreadInput(target_thread) ->
      SetForegroundWindow -> detach` (um SetForegroundWindow simples de um processo
      background e recusado pelo foreground-lock do Windows). Verifica
      `GetForegroundWindow() == target_hwnd`; se nao bater, aborta.
   c. **Cola.** Poe o texto refinado no clipboard, injeta Ctrl+V (virtual-key codes).
   d. **Restaura o clipboard anterior** SO depois de confirmar (via sequence number) que
      a colagem foi consumida, e so se nenhum terceiro mexeu no clipboard entretanto.
10. **Cancelar/Esc/erro.** Corre o MESMO restore gated do clipboard que o caminho de
    sucesso (nunca deixa o clipboard no estado refinado). Esconde a overlay.

Reentrancia: se a hotkey for premida de novo com um loop em curso, ignora ou reinicia
de forma controlada (guard de "loop in flight").

## 5. Camada nativa: captura e substituicao

A parte mais arriscada. Crates e tecnicas verificadas (Tauri 2.x, Windows, jun 2026).

- **Hotkey**: `tauri-plugin-global-shortcut` 2.3.x (+ companion JS). Default uncommon
  e **configuravel**. `RegisterHotKey` falha se a combo ja estiver tomada: trata o
  `Result` e mostra a falha na UI, nunca silenciosa. Capability `global-shortcut:default`.
- **Overlay nao-ativavel**: `WebviewWindowBuilder` com `.decorations(false)
  .transparent(true).always_on_top(true).skip_taskbar(true).focusable(false)
  .shadow(false).resizable(false).visible(false)`. O `focusable(false)` e load-bearing:
  o backend Windows do tao traduz a ausencia da flag FOCUSABLE em `WS_EX_NOACTIVATE`,
  pelo que a overlay nunca ativa ao clicar e o app original mantem foco. Preferir o
  builder a `tauri.conf.json "focus": false` (este esta reportado partido no Windows,
  issue #11566). Re-chamar `set_always_on_top(true)` em cada show (cai apos hide/show,
  issue #13530). Belt-and-suspenders: forcar `WS_EX_TOOLWINDOW` via `hwnd()` se persistir
  na taskbar.
- **Injecao de teclas**: `enigo` 0.6.1. CRITICO: para a letra usar VIRTUAL KEY, nao
  `Key::Unicode`. No Windows `Key::Unicode('v')` vai por `KEYEVENTF_UNICODE`, que NAO
  honra o modificador Ctrl, e o Ctrl+V faz no-op silencioso em muitas apps. Usar
  `Key::Other(0x56)` (VK_V) e `Key::Other(0x43)` (VK_C). Pequenos delays inter-evento
  para a fila de mensagens do alvo processar o modificador antes da letra.
- **Clipboard**: `arboard` 3.4.x (snapshot/set/restore) como primario, com
  `tauri-plugin-clipboard-manager` 2.3.x como caminho alternativo (resiliencia).
  Deteccao de fim de copia via `GetClipboardSequenceNumber()` ou
  `AddClipboardFormatListener` -> `WM_CLIPBOARDUPDATE` (nunca sleep fixo). Retry de
  `OpenClipboard` em `ERROR_ACCESS_DENIED` com backoff. Save/restore de todos os
  formatos best-effort (`EnumClipboardFormats`); formatos delay-rendered/owner podem
  ser irrecuperaveis e isso documenta-se.
- **Foco/foreground**: `AttachThreadInput` para restaurar foco antes de colar (ver 4.9b).

## 6. Degradacao honesta (alvos nao suportados)

Aplicacao do standard de resiliencia a camada do SO: detetar e recusar com mensagem
clara, nunca corromper em silencio.

| Alvo | Problema | Comportamento |
|---|---|---|
| App elevada/admin | UIPI bloqueia SendInput e clipboard | Detetar integrity level do processo alvo. "Nao posso refinar em janelas admin." Nao correr elevado em v1. |
| Terminal/console | Ctrl+C = interrupt, nao copiar | Detetar classe (ConsoleWindowClass, CASCADIA). Recusar (ou paste especifico do terminal). |
| Fullscreen-exclusive/jogo | Overlay nao renderiza, input raw ignora SendInput | Detetar e abortar com mensagem. |
| Secure desktop (UAC/lock) | Input isolado | Out of scope, falhar fechado. |
| Nada selecionado | Refinaria clipboard antigo | Caminho da caixa propria (seccao 4.4). |
| Ctrl+Z inconsistente no alvo | Web/Electron gravam paste de forma irregular, terminais nao tem undo | Nao depender do undo do alvo. Guardar o original e oferecer "Revert" explicito no widget. |

## 7. Nucleo de refinamento (qualidade)

System prompt que:
- Preserva a intencao do utilizador.
- Escala a agressividade (pergunta curta -> polir clareza/wording; tarefa -> estrutura
  com papel, contexto, constraints, formato de output).
- Corrige ortografia, acentos e cedilhas na lingua detetada.
- Deteta a lingua do input e responde na MESMA lingua (multilingue), salvo override do perfil.
- Aplica as regras do perfil (ex: sem em-dashes, tom).
- Devolve SO o prompt refinado, sem preambulo.

O construtor do prompt (input + perfil -> system prompt final) e uma funcao pura testavel.

## 8. Perfil de personalizacao

- **Auto-deteccao**: procura um CLAUDE.md na maquina (ex: `%USERPROFILE%\.claude\CLAUDE.md`;
  CLAUDE.md de projeto se detetavel). Se encontrar, usa-o e fica pessoal automaticamente.
- **Painel editavel** nas settings: mostra o perfil detetado, permite editar/override.
- **Perfil "qualidade" por defeito** para quem nao tem ficheiro nenhum (torna o app
  agnostico e util para qualquer consumidor).
- O resolutor de caminhos do perfil e uma funcao pura testavel.
- Nota de custo: o CLAUDE.md pode ser grande; injetar so a parte de estilo/tom relevante
  (ou o ficheiro inteiro como contexto cacheado). Prefixo estavel para prompt caching.

## 9. Providers e resiliencia

BYOK. Primario Gemini Flash, fallback Claude (familias diferentes, falham por razoes
diferentes). Satisfaz `provider-fallback-on-transient-errors` / STACK-07.

**Gemini (primario)**
- Modelo default: `gemini-2.5-flash` (GA, melhor price-performance, free-tier-eligible),
  editavel nas settings. Upgrades opt-in: `gemini-flash-latest`, `gemini-3.5-flash`.
  Lite: `gemini-2.5-flash-lite`. NAO usar `gemini-2.0-flash` (desativado 2026-06-01).
- Endpoint: `POST https://generativelanguage.googleapis.com/v1beta/models/{MODEL}:generateContent`
  (preview ao vivo: `:streamGenerateContent?alt=sse`).
- Auth: header `x-goog-api-key` (nunca `?key=` na URL).
- Rate limits: NAO hardcoded. HTTP 429 (RESOURCE_EXHAUSTED) e a fonte de verdade em
  runtime -> backoff e/ou fallback. (Limites exatos vivem no dashboard do AI Studio e
  variam por regiao/conta; muitos utilizadores BYOK ja estarao em Tier 1+.)

**Claude (fallback)**
- Default: `claude-sonnet-4-6` (melhor equilibrio velocidade/inteligencia). Opcoes:
  `claude-haiku-4-5` (mais rapido/barato), `claude-opus-4-8` (qualidade maxima).
- Endpoint: `POST https://api.anthropic.com/v1/messages`.
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`, `Content-Type: application/json`.
  (`anthropic-version` e obrigatorio.)

**Taxonomia de erros**
- Retryable (transient): timeouts/connection, 408, 409, 429 (honrar Retry-After/RetryInfo),
  500, 503, 504, 529 (overloaded).
- Propagar sem mascarar: 400 (payload), 413 (too large), 404 (modelo invalido).
- Auth (401/403): NAO retry cego; classe AUTH -> tenta fallback (chave diferente).
- Content-policy: Anthropic 200 + `stop_reason="refusal"`; Gemini 200 +
  `promptFeedback.blockReason` / `finishReason=SAFETY|RECITATION`. Nao-transitorio,
  propaga por defeito (flag opcional `fallbackOnContentPolicy` para tentar a outra familia).

**Control flow puro e testavel**
- `classify(providerKind, httpStatus, apiErrorCode?, retryAfterMs?) -> {class, retryAfterMs?}`
- `backoffMs(attempt, cfg, rng01, serverRetryAfterMs?) -> number` (rng01 injetado em [0,1),
  sem Math.random/Date.now no interior -> testes deterministicos)
- `plan(state, outcome, cfg, rng01) -> Decision` onde Decision e
  `succeed | retry(delayMs, nextState) | fallback(nextState) | fail(reason)`.
- Orquestrador impuro (~15 linhas): so `callProvider`, `sleep`, `rng()`, `now()` sao
  efeitos; toda a ramificacao vive nas funcoes puras. Cap de wall-clock global para um
  refine interativo continuar rapido mesmo degradado. Em esgotamento total: manter a
  selecao original intacta e mostrar erro honesto, nunca colar nada.

**Pre-validacao (nao-negociavel)**
- Validar AMBAS as chaves a entrada (probe barato), nao no momento da falha. Gemini:
  `GET .../v1beta/models` com `x-goog-api-key`. Anthropic: check de 1 token. Sem fallback
  valido: degradar visivelmente (toast + estado "primario apenas, sem fallback"), nunca
  em silencio. Probes feitos do Rust.

## 10. Privacidade e seguranca

- **Fronteira de dados**: o texto selecionado pode ser password/token/PII e vai para um
  LLM terceiro. Tornar o destino obvio na UI; considerar mascarar campos de password
  conhecidos; o envio so acontece por accao explicita (a hotkey).
- **Chaves**: `keyring` 3.6.x com feature `windows-native` OBRIGATORIA (sem ela, mock
  store volatil silencioso). Cada chave = um Generic Credential no Windows Credential
  Manager. Self-test no arranque que escreve+le um probe e avisa se a persistencia nao
  for real. NAO saltar para keyring v4. Sem chaves em plaintext nem no git.
- **Sem telemetria** por defeito. Self-host de fontes (Geist) e icones (Phosphor)
  bundled: funciona offline e nao faz phone-home (uma ferramenta de clipboard tem de
  ser privada).
- **Code-signing**: um background app que regista hotkey global, simula teclas e le o
  clipboard e comportamentalmente identico a um keylogger; assinar o binario reduz
  friccao com AV/EDR/SmartScreen (nao elimina heuristicas).

## 11. UI premium

**Stack**
- Animacao: **Motion** (`motion/react`, v12+, ex-Framer Motion). `layoutId` partilhado
  faz o morph orb->bolha praticamente de graca (FLIP, so transforms); springs
  interrompiveis para estados que mudam depressa; motor hibrido corre transform/opacity
  na WAAPI (off main thread).
- Componentes (settings): **shadcn/ui** sobre Radix + **Tailwind v4** + React 19, tema
  via CSS variables que mapeiam 1:1 nos tokens abaixo. So o necessario: Dialog/Sheet,
  Tabs, Switch, Select, Input+Label, Slider, Tooltip, Sonner (toasts), Form
  (react-hook-form + zod para validar chaves). Acessibilidade vem dos primitives Radix.
- Bolha: micro-controlos bespoke (`motion.button` Accept/Retry/Cancel) com `<button>`
  reais, focus ring no accent, aria-labels, foco preso enquanto aberta.
- Icones: **Phosphor** (`@phosphor-icons/react`), `weight="duotone"` premium e `"fill"`
  para activo. Uma so familia.
- Tipografia: **Geist Sans** (variavel) + Geist Mono para a area de output (monospace
  torna edicoes a nivel de caracter legiveis). Self-hosted woff2, sem CDN.

**Arquitetura da overlay (perf 60fps em WebView2)**
- UMA janela transparente fixa (tamanho da bolha maxima); morph e CSS, nao resize do SO.
- Click-through: loop Rust ~60Hz que faz toggle de `set_ignore_cursor_events(true/false)`
  consoante o cursor esta sobre o orb/bolha (opaco) ou sobre o canvas transparente.
- Animar SO `transform` e `opacity` (compositam no GPU). Nunca width/height/top/left/
  box-shadow-blur/backdrop-filter por frame.
- O painel de vidro (backdrop-blur) e o item mais caro: mante-lo ESTATICO e animar o
  orb/glow/conteudo como camadas irmas por cima. Glow = box-shadows empilhadas cuja
  OPACIDADE se anima (nao o raio de blur). No maximo uma superficie de vidro grande visivel.

**O morph (orb <-> bolha)**
- Mesmo `layoutId="refiner-surface"` no orb e na bolha, dentro de `<AnimatePresence
  mode="popLayout">`. Transition: `{ type:"spring", stiffness:420, damping:34, mass:0.9 }`
  (~450ms percebidos). Orb `borderRadius:9999`, bolha `20`. `borderRadius` inline para
  Motion auto-corrigir; `layout` nos filhos diretos para counter-scale. Conteudo da bolha
  resolve com `delay:0.06` depois da forma (o staging e o que parece premium).

**Estados do orb**
- IDLE: glow ambiente lento (sempre "vivo").
- REFINING: respiracao `scale:[1,1.06,1]` infinita + anel conic-gradient a rodar.
- SUCCESS: pop `scale:[1,1.18,1]` spring, flash verde, checkmark via `pathLength 0->1`.
- ERROR: shake `x:[0,-8,8,-6,6,-3,0]` + flash vermelho, volta a laranja.
- `<MotionConfig reducedMotion="user">` global para honrar prefers-reduced-motion.

**Tokens de design** (dark glassy, ancorado em laranja quente)
- Accent: `#FF7A18` (hover `#FF8F3C`, active `#E8650A`), texto sobre accent `#1A0E03`
  (branco-sobre-laranja falha AA). Orb gradient radial: `#FFE6BE -> #FF9A3D -> #FF6A00
  -> #D9510A`.
- Backgrounds: canvas `#0B0A09`, settings `#111014`. Surfaces: `#16151A`, inputs
  `#1F1D24`, hover `#26232C`.
- Borders: subtle `rgba(255,255,255,0.08)`, default `0.12`, strong `0.18`, accent/focus
  `rgba(255,138,60,0.55)`.
- Texto: primary `#F5F3F0` (off-white quente), secondary `#ABA59E`, muted `~0.42`,
  on-accent `#1A0E03`.
- Semantico: success `#34D399`, warning `#FFCB47` (lemon-shifted, sempre com icone +
  texto para nao se ler como accent), error `#FF5D5D`, info `#4DA3FF`.
- Vidro: bubble `rgba(20,18,24,0.55)` + `backdrop-filter: blur(20px) saturate(140%)`;
  fallback opaco `rgba(14,13,17,0.92)` via `@supports not (backdrop-filter)`. Painel
  settings `blur(16px) saturate(130%)`.
- Radius: xs6, sm10, md14, lg20 (bolha), xl28 (settings), full9999 (orb). Morph anima full->20.
- Glow assinatura: `0 0 0 1px rgba(255,138,60,0.40), 0 0 24px 4px rgba(255,122,24,0.45),
  0 0 64px 12px rgba(255,106,0,0.30)` (animar opacidade, nao blur).
- Motion tokens: spring-morph {420,34,0.9}, spring-pop {600,18}, spring-soft {260,30};
  ease-standard `cubic-bezier(0.22,1,0.36,1)`; fast140/base220/slow360; stagger 60ms.

**Marca (Ember)**
- O orb ao vivo E o logo (produto e marca sao o mesmo orb laranja).
- App icon (SVG full-color): tile rounded-square, fundo radial `#1A1410 -> #0B0A09`,
  orb com gradient radial, halo `#FF7A18 @0.55` com blur, especular branco top-left,
  uma sparkle de 4 pontas (sinal de "refine/magic").
- Tray icon (mono, legivel a 16-24px): anel 2px + core dot + sparkle, branco-on-transparent
  (Win11 tinge) + variante laranja. SVG:
  `<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="6.5" fill="none" stroke="#FFF"
  stroke-width="2"/><circle cx="12" cy="12" r="2.5" fill="#FFF"/><circle cx="18.5"
  cy="5.5" r="1.25" fill="#FFF"/></svg>`. Rasters .ico/.png (16/24/32/48/256) para a tray/exe.

**Modelo de teclado** (bolha aberta): Enter = aceitar (colar in-place), Esc = cancelar,
R = refinar de novo, C = copiar. Tooltips Radix nos hints. Como a overlay e nao-ativavel,
registar estes atalhos como window/global shortcuts para chegarem a bolha sem roubar
foco ao alvo antes da hora.

## 12. Testes

- **Funcoes puras (Rust)**: `classify`, `backoffMs`, `plan` (table-driven, sem rede);
  normalizacao de modificadores presos; resolutor de caminhos do perfil; construtor do
  system prompt; deteccao transitorio/nao-transitorio.
- **Golds**: uns prompts representativos por arquetipo (pergunta curta, tarefa complexa,
  texto com erros de acento) com baseline fixado, para apanhar regressoes de qualidade
  (`gold-examples-as-regression-bar`).
- **Matriz manual** (nao automatizavel): o loop captura->colar em apps reais (browser,
  VS Code, Word, terminal=deve recusar, app admin=deve degradar), em mono e dual-monitor
  mixed-DPI (ex: 100% + 150%).
- **Componentes**: estados do orb e accept/reject da bolha.

## 13. Ambito v1 vs depois (YAGNI)

**v1** (inclui honestamente os ramos de degradacao):
- Loop magico endurecido (foco, clipboard gated, modificadores, re-validacao, revert).
- Refinamento adaptativo, multilingue, acentos.
- Perfil auto-detetado + editavel + default.
- Gemini + Claude BYOK com a maquina de resiliencia pura e pre-validacao.
- Deteccao + degradacao honesta de alvos nao suportados.
- UI premium (orb, morph, estados, tokens, settings, tray), marca Ember.
- Storage seguro de chaves, self-host de fontes/icones.
- Instalador Windows assinado.

**Depois (opcional)**: historico de refinamentos, multiplos perfis/presets, toggle
Polir/Turbo na bolha, regras por-app, biblioteca de templates, mascaramento avancado de
campos sensiveis, fidelidade total de clipboard non-text, suporte a apps elevadas
(uiAccess), sync, macOS/Linux.

## 14. Riscos abertos / decisoes pendentes

A confirmar em hardware real ou por decisao de produto:
1. Overlay Tauri 2.x mostrar mesmo sem roubar foco (historicamente finicky): validar cedo.
2. Fidelidade de clipboard non-text (imagens/ficheiros): suportar best-effort ou
   documentar a perda. Decisao de produto.
3. Profundidade do mascaramento de campos de password: deteccao real e dificil; v1 pode
   so tornar o destino obvio.
4. Apps elevadas: confirmar que v1 degrada (nao corre elevado).
5. Re-verificar IDs de modelos Gemini GA atuais e limites do dashboard no momento do build.
6. Custo de tokens do perfil grande: medir antes de otimizar (injetar so o relevante vs
   ficheiro inteiro cacheado).

## 15. Dependencias concretas

Rust: `tauri` 2.x (feature `tray-icon`), `tauri-plugin-global-shortcut` 2.3.x,
`tauri-plugin-clipboard-manager` 2.3.x, `arboard` 3.4.x, `enigo` 0.6.1,
`keyring` 3.6.x (feature `windows-native`), `windows` ~0.58 (Win32: foreground,
clipboard seq, integrity level), `reqwest`, `serde`/`serde_json`, `tokio`,
`raw-window-handle` 0.6.

Frontend: React 19, Vite, Tailwind v4, `motion`, shadcn/ui (Radix), `@phosphor-icons/react`,
Geist (woff2 self-hosted), react-hook-form + zod, Sonner.

App: manifest PerMonitorV2 DPI awareness. Instalador via tauri bundler, assinado.

## 16. Mapeamento ao playbook

- `provider-fallback-on-transient-errors` / STACK-07 / `resilience-fallback-or-degrade`:
  Gemini primario + Claude fallback, retry transitorio + fallback no esgotamento,
  nao-transitorio propaga, control flow puro testavel, fallback pre-validado, degradacao
  honesta e visivel (estendida a camada do SO).
- standards/04-security: chaves no Credential Manager, fora do git, sem telemetria,
  code-signing.
- standards/05-quality: golds como barra de regressao, Conventional Commits.
- `dont-reinvent-the-wheel`: reuso de Tauri plugins/crates e libs maduras (Motion,
  shadcn, Phosphor) em vez de codigo novo.
- `measure-before-you-optimize`: custo de tokens do perfil e FPS do morph medidos em
  hardware real antes de otimizar.
- `mobile-first-non-negotiable`: N/A (desktop overlay Windows; excecao documentada).
- Tom/output: refinador respeita "sem em-dashes", multilingue, perfil do utilizador.
