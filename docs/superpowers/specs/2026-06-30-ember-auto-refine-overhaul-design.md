# Ember: auto-refine in-place + overhaul (design)

Data: 2026-06-30
Estado: aprovado (brainstorming). Próximo passo: plano de implementação.

## 1. Contexto e objetivo

O Ember v0.1 abre uma janela ao centro com uma caixa manual onde se escreve/cola o
prompt, refina, e mostra preview com accept/reject/retry/copy. Não é o que se quer.

Objetivo do overhaul: tornar o Ember **invisível e instantâneo**. Selecionas texto em
qualquer app, carregas no hotkey, aparece um orb a carregar **junto ao cursor**, e o
texto selecionado é **refinado e substituído in-place automaticamente**. Sem janela ao
centro, sem caixa manual, sem botões.

Mais: logo novo (abstrato/profissional), **toda a UI em inglês**, restyle premium das
Settings, e varredura de typos.

## 2. Decisões aprovadas

| Tema | Decisão |
|---|---|
| Sem seleção ao premir o hotkey | **Hint subtil** junto ao cursor ("Select text first"). A caixa manual é removida por completo. |
| Modo de substituição | **Totalmente automático**: substitui logo, sem confirmação (o Ctrl+Z da app faz undo). |
| Logo | **"Ember arc"**: core com glow + arco orbital offset. |
| Âmbito visual | **Restyle completo**: logo + comportamento + inglês + Settings repolidas. |
| Simulação de input | **enigo** (Ctrl+C/Ctrl+V + posição do cursor). |
| Clipboard | **arboard** (síncrono, deteta não-texto) no módulo nativo. |

## 3. Arquitetura e fluxo de dados

O pipeline de refino (`ember-core` + `providers::refine`, com retry transitório e
fallback Gemini→Claude) **mantém-se intacto**. O que muda é a *entrada* (texto vem da
seleção, não de uma caixa) e a *saída* (substitui a seleção, não mostra preview).

### Fluxo "fully automatic"

1. **Hotkey premido** (callback do global-shortcut, main thread):
   - lê posição do cursor (`enigo`);
   - posiciona e mostra o orb (janela `overlay` pequena, transparente, **sem foco**) junto
     ao cursor, com offset e clamp à área de trabalho do monitor atual;
   - emite estado `refining`;
   - lança o trabalho async (`tauri::async_runtime::spawn`).

2. **Capturar a seleção sem destruir o clipboard (técnica do sentinela):**
   - guarda o clipboard atual `S` (texto; se não-texto, marca como não-restaurável-como-texto);
   - **liberta os modificadores do hotkey** (Ctrl/Shift) e pequeno delay, para o
     Ctrl+Shift+Space não contaminar a simulação seguinte;
   - escreve um **sentinela único** no clipboard;
   - simula **Ctrl+C**;
   - faz poll do clipboard até ~300ms:
     - continua = sentinela → **nada selecionado** → restaura `S`, emite `hint`, desvanece;
     - mudou → o conteúdo é o texto selecionado `T`.

3. **Refinar `T`** com `providers::refine` (chain Gemini→Claude, igual ao atual).

4. **Substituir (sucesso):**
   - escreve `R` (refinado) no clipboard;
   - simula **Ctrl+V** (a app em foco recebe; o orb não rouba foco);
   - delay ~80ms para o paste assentar;
   - **restaura `S`**;
   - emite `success` (flash breve), desvanece.

5. **Erro:** emite `error` (indicador vermelho + mensagem curta junto ao cursor); se a
   causa for "sem chave configurada", abre as Settings. **Restaura `S` sempre.**

### Sequenciamento e threads

- `capture_selection()` e `replace_selection()` são **síncronos** (enigo + arboard + sleeps
  curtos). Correm em `spawn_blocking` para não bloquear o runtime; o refino async é
  aguardado entre os dois.
- O clipboard é **sempre restaurado**, inclusive nos ramos de erro (RAII/guard).

## 4. Componentes

### Rust (`src-tauri`)

- **Novo `selection.rs`** (a peça central, testável):
  - `capture_selection(io) -> SelectionOutcome` onde `SelectionOutcome ∈ { Captured(String), Empty }`.
  - `replace_selection(io, refined)`.
  - O sequenciamento (sentinela → poll → deteção de "vazio" → restauro) é uma **função pura**
    sobre um trait `ClipboardIo + InputIo`, com implementação real (arboard/enigo) e **fakes**
    para teste sem rede/SO. (prática: control flow de fallback testável.)
- **`lib.rs`**: o callback do hotkey deixa de chamar `show_overlay_manual`; passa a:
  posicionar o orb no cursor (sem foco) → orquestrar capture → refine → replace. Tray em
  inglês ("Settings", "Quit"). `show_overlay_manual`/`hide_overlay` substituídos por
  `show_orb_at_cursor`/`hide_orb`.
- **`commands.rs`**: remove o fluxo manual e accept/reject/retry/copy (`submit_manual`,
  `retry_refinement`, `accept_refinement`, `reject_refinement`, `copy_refinement`). O refino
  passa a ser invocado pelo loop nativo, não por comando JS. Mensagens de erro em inglês.
  Settings commands mantêm-se (get_settings, set_model, set_hotkey, set_autostart,
  set_api_key, clear_api_key, validate_key, set_profile, reload_profile, reset_profile).
- **`state.rs`**: `Pending` deixa de ser preciso (sem preview). Mantém-se `http`.
- **`tauri.conf.json`**: janela `overlay` passa a pequena (~260x100), continua transparent,
  no-decorations, always-on-top, skipTaskbar, **focus:false**, shadow:false.
- **`Cargo.toml`**: adiciona `enigo` e `arboard`; `windows` só se for preciso forçar
  `WS_EX_NOACTIVATE`. O `tauri-plugin-clipboard-manager` deixa de ser usado no hot path
  (pode sair, com a capability respetiva).
- **Capabilities**: `overlay.json` deixa de precisar de `clipboard-manager`; mantém
  `global-shortcut`, `positioner` (ou posicionamento manual via set_position).

### Frontend (`src`)

- **Overlay reescrito** para feedback junto ao cursor, estados:
  - `refining` → orb (anel a rodar + pulse), já existente em `Orb.tsx`;
  - `success` → tick breve (≈600ms) e fade;
  - `error` → pílula glass pequena, vermelha, com mensagem curta;
  - `hint` → pílula glass "Select text first" (substitui o antigo `empty`).
  - Remove `Bubble.tsx` (preview/manual) e o modelo de teclado Enter/Esc/R/C do controller.
  - `types.ts`: fases passam a `refining | success | error | hint | hidden`. Sai `preview`/`empty`.
- **Settings**: **inglês** + restyle premium (tipografia, espaçamento, cards com borda/realce
  mais finos, motion subtil nas tabs). Tabs: **Providers, Shortcut, Profile, Appearance, About**.
  Todas as strings, toasts e placeholders em inglês.
- **Logo "Ember arc"** como componente SVG reutilizável (`src/components/Logo.tsx` ou similar):
  usado no header das Settings e como favicon das páginas. O orb de loading mantém a sua
  animação (anel+core), alinhado com a nova identidade.

### Logo e ícones

- Desenhar o "Ember arc" em SVG (core radial laranja `--orb-gradient` + arco orbital com o
  accent; glow via box-shadow/filter coerente com `--orb-glow`).
- Exportar um PNG ≥1024px do mark e correr `npm run tauri icon <png>` para regenerar
  `icons/*.png`, `icon.ico`, `icon.icns` (app + tray passam a usar o novo mark).
- O render SVG→PNG faz-se com uma ferramenta headless (ex.: resvg/sharp ou via browser).

## 5. Tratamento de erros e edge cases

| Caso | Comportamento |
|---|---|
| Nada selecionado | hint "Select text first" junto ao cursor, fade; clipboard restaurado. |
| Sem chave configurada | erro "No API key. Opening settings…" + abre Settings; clipboard restaurado. |
| Providers falham (rede/limites) | erro "Couldn't refine. Try again." ; clipboard restaurado. |
| Auth inválida | erro "Invalid API key. Check settings." |
| Content policy | erro "Blocked by the provider's content policy." |
| Clipboard original era imagem/ficheiros | restauro best-effort (só texto); nota conhecida v1. |
| Modificadores do hotkey ainda premidos | libertar modificadores + delay antes do Ctrl+C sintético. |
| Orb a roubar foco | `focused(false)`; se necessário, `WS_EX_NOACTIVATE` via crate `windows`. |

## 6. i18n + typos

- Substituir **todas** as strings PT por inglês: Settings (tabs, secções, hints, labels,
  botões, placeholders), toasts, tray menu, mensagens de erro do core (`friendly_error`),
  header/subtítulo, About.
- Varredura de typos no texto novo em inglês e no existente (README incluído).
- Comentários de código podem ficar em PT (não são user-facing), mas corrige typos óbvios.

## 7. Testes

- `selection.rs`: testes do sequenciamento com fakes (sem SO/rede):
  - sentinela inalterado → `Empty`;
  - sentinela alterado → `Captured(T)`;
  - restauro de `S` em sucesso e em erro;
  - não-restaurável-como-texto não rebenta.
- Pipeline de refino: lógica já coberta em `ember-core` (classify/plan/retry); mantém-se.
- Verificação manual (skill `run`): selecionar texto numa app, hotkey, ver orb no cursor e
  o texto substituído; testar "sem seleção" e "sem chave".

## 8. Riscos e mitigações

- **Race de timing do clipboard/keystrokes**: poll com timeout + sentinela (robusto a
  conteúdo igual); delays calibrados; libertar modificadores do hotkey.
- **Foco da janela orb**: `focused(false)` + (se preciso) no-activate nativo.
- **AV/heurística** (Ctrl+C/Ctrl+V sintéticos): padrão legítimo (PowerToys, TextBlaze);
  sem captura global de teclado, só simulação pontual.
- **DPI/multi-monitor**: usar pixels físicos consistentes (enigo + PhysicalPosition) e
  clamp à área de trabalho do monitor atual.

## 9. Fora de âmbito (YAGNI)

- Preview/undo custom (o Ctrl+Z da app chega).
- Restauro de clipboard não-texto (imagem/ficheiros) para lá do best-effort.
- Temas adicionais / modo claro.
- macOS/Linux (enigo deixa a porta aberta, mas v1 é Windows).
