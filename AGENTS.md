# Ember: instruções para agentes

O guia completo é o [CLAUDE.md](CLAUDE.md); este é o essencial condensado.

Ember (Tauri v2, Windows-first) refina texto selecionado em qualquer app: hotkey global,
captura via clipboard, LLM, paste de volta.

## Estrutura

- `crates/ember-core/`: lógica pura, sem I/O nem rede, toda com testes unitários. Prompts,
  retry/fallback, modelos, projetos, hotkeys.
- `src-tauri/`: shell Tauri com todo o I/O (janelas, hooks de teclado, clipboard, HTTP, OAuth,
  segredos). Lógica decidível sem I/O desce para o core.
- `src/`: React 19 + Tailwind + motion; entradas settings/overlay/picker/splash.

## Regras que não se negoceiam

- Overlay e picker nunca recebem foco; o paste tem de aterrar na app do utilizador.
- Hooks de teclado LL só consomem a tecla sobre a qual agem (mais a cauda dela); tudo o resto
  passa. Um hook de cada vez: `stop_and_join()` antes de instalar o seguinte.
- Chaves de API ficam no Windows Credential Manager; nunca chegam ao JS.
- Conteúdo de repos de utilizador entra no prompt como dado embrulhado em markers anti-injeção;
  a destilação nunca faz fallback para conteúdo cru.
- Fallbacks de modelo só entre modelos gratuitos.

## Verificar e construir

- `cargo test --workspace` e `npx tsc --noEmit` antes de dar algo por feito.
- `npm run tauri build` gera o instalador NSIS; `/S` instala silencioso.
- Debugging começa no log: `%LOCALAPPDATA%\com.deleg8lab.ember\logs\Ember.log`.

## Estilo

- Conventional Commits; comentários em português a explicar o porquê; zero crédito a IA em
  commits e PRs.
