# Ember: guia para agentes de código

Ember é uma app Windows-first (Tauri v2) que refina texto selecionado em qualquer aplicação:
hotkey global, captura via clipboard, LLM, paste de volta. Este ficheiro é o canónico; o
AGENTS.md resume o mesmo para outras ferramentas.

## Mapa do workspace

- `crates/ember-core/`: o cérebro, **puro**. Zero I/O, zero rede, zero Tauri. Prompts, retry e
  fallback, classificação de erros, seleção de modelos, projetos, hotkeys, teclado do picker.
  Tudo aqui tem testes unitários (`cargo test --workspace`, correm em milissegundos).
- `src-tauri/`: o shell. Todo o I/O vive aqui: janelas, hooks de teclado/rato, clipboard, HTTP,
  config, segredos, OAuth. Regra dura: lógica decidível sem I/O desce para o ember-core e ganha
  testes; o shell orquestra.
- `src/`: frontend React 19 + Tailwind + motion. Quatro entradas: `settings.html`,
  `overlay.html`, `picker.html`, `splash.html` (ver `vite.config.ts`).

## Regras invioláveis (cada uma existe por causa de um bug real)

1. **A overlay e o picker NUNCA recebem foco** (`focus: false`, nunca chamar `set_focus`). O
   paste tem de aterrar na app do utilizador; roubar o foco uma vez mata todos os refines.
2. **Hooks de teclado low-level nunca engolem teclas do utilizador.** Consome-se apenas a tecla
   sobre a qual se age, mais a cauda dela (auto-repeat e key-up, ver `drain_until_released`).
   Qualquer outra tecla passa intocada. Esta regra está fixada em testes; não a negoceies.
3. **Um hook LL de cada vez.** Antes de instalar um segundo (ex.: gate de preview depois do
   watcher de Esc), `stop_and_join()` no primeiro. Dois hooks vivos a consumir a mesma tecla é
   corrupção de input.
4. **Chaves de API nunca cruzam a fronteira JS.** Vivem no Windows Credential Manager
   (`secrets.rs`, crate `keyring`). O frontend só sabe `hasGeminiKey: bool`.
5. **Conteúdo de projeto é dado, nunca instrução.** Os markers `[EMBER_PROJECT_CONTEXT]` /
   `[EMBER_PROJECT_SOURCE]` embrulham texto de repos alheios com preâmbulo anti-injeção. A
   destilação de um brief nunca faz fallback para o conteúdo cru do ficheiro.
6. **Nunca gastar dinheiro que o utilizador não escolheu.** Fallbacks de modelo só entre
   modelos gratuitos; caminhos pagos são escolha explícita.
7. **Resiliência**: retry com backoff no transitório, família diferente só no esgotamento,
   erros não-transitórios (auth, payload, content-policy) propagam sem máscara. O control flow
   é puro (`retry.rs`) e testado sem rede.

## Comandos

```bash
cargo test --workspace          # a partir de src-tauri/ ou da raiz
npx tsc --noEmit                # type-check do frontend
npm run tauri build             # gera o NSIS em target/release/bundle/nsis/
```

Instalar silencioso: `Ember_*_x64-setup.exe /S`. O exe fica em `%LOCALAPPDATA%\ember\ember.exe`.
O build queixa-se de `TAURI_SIGNING_PRIVATE_KEY` em builds locais; o instalador sai na mesma
(a assinatura só importa para o updater em releases).

## Onde vive o quê em runtime

- Config: `%APPDATA%\com.deleg8lab.ember\config.json` (nunca segredos).
- Segredos: Windows Credential Manager.
- Logs: `%LOCALAPPDATA%\com.deleg8lab.ember\logs\Ember.log`. Debugging começa SEMPRE aqui: o
  código loga decisões (gate, picker, retry, geometria) precisamente para não se adivinhar.
- Prompts: `prompts.jsonl` ao lado dos logs, **opt-in e default off** (leva texto do utilizador
  para disco).

## Convenções

- Conventional Commits (release-please gera o CHANGELOG a partir deles).
- Comentários de código em português, a explicar o PORQUÊ, com o bug que motivou a regra.
- Zero crédito a ferramentas de IA em commits, PRs ou comentários. Sem trailers de co-autoria.
- Constantes espelhadas entre Rust e TS (ex.: geometria do picker) declaram o espelho num
  comentário nos dois lados; muda um, muda o outro.
- macOS: paridade documentada em `docs/macos-parity.md`; o caminho Windows é o primário.
