# Relatório de Auditoria de Segurança — Ember

**Data:** 01 de Setembro de 2026  
**Âmbito:** Codebase completa (Rust Core `crates/ember-core`, Tauri v2 Shell `src-tauri`, React Frontend `src/`), Histórico Git e Dependências.  
**Estado Geral:** **SEGURO** com boa arquitetura defensiva; 2 melhorias de sanitização e atualizações de dependências identificadas.

---

## 1. Sumário Executivo

A auditoria cobriu a análise de arquitetura, varredura de segredos no histórico Git, isolamento de credenciais, injeção de comandos/prompts, ganchos de teclado (low-level hooks) e análise de vulnerabilidades na cadeia de dependências (`cargo audit` e `npm audit`).

### Matriz de Vulnerabilidades e Riscos

| ID | Área | Severidade | Descrição | Estado |
|---|---|---|---|---|
| **SEC-01** | Prompts / LLM | **Médio** | Falta de escape dos delimitadores de projeto (`[/EMBER_PROJECT_SOURCE]` e `[/EMBER_PROJECT_CONTEXT]`) | Mitigação recomendada |
| **SEC-02** | Supply Chain | **Alto** (Transitivo) | Vulnerabilidades conhecidas em `quick-xml`, `rkyv`, `postcss` e `nanoid` | Corrigível via update/audit fix |
| **SEC-03** | Shell / OS | **Baixo / Info** | Invocação do browser via `cmd.exe /C start` em Windows em vez de `ShellExecuteW` | Melhoria de robustez |
| **SEC-04** | Privacidade | **Informativo** | Armazenamento de prompts em disco quando `savePrompts: true` (opt-in) | Documentado / Por design |

---

## 2. Varredura de Segredos e Histórico Git / GitHub

- **Scan de Commits:** Foram analisados todos os commits e diffs do histórico Git (`git log --all -p`).
- **Resultado:** **Zero chaves de API, credenciais ou tokens expostos.**
- **Armazenamento de Segredos:**
  - As chaves de API (Gemini, OpenAI, OpenRouter, Groq, etc.) são armazenadas **exclusivamente** no Windows Credential Manager (via crate `keyring` com a feature `windows-native`) e na Keychain no macOS (`apple-native`).
  - Nenhuma chave é enviada para o frontend JavaScript via IPC (a UI recebe apenas `hasGeminiKey: bool`, `hasOpenAiKey: bool`).
  - O fluxo OAuth do ChatGPT utiliza PKCE com gerador de números aleatórios criptográfico do SO (`getrandom`), hash SHA-256 e validação rigorosa de CSRF `state`. O `access_token` é volátil (vive apenas em memória em `AppState.oauth_access`) e apenas o `refresh_token` é persistido no cofre de credenciais.

---

## 3. Análise Detalhada de Vulnerabilidades

### SEC-01: Potencial Escape de Delimitadores em Ficheiros de Projeto (Prompt Injection)
- **Ficheiros afetados:**
  - `crates/ember-core/src/prompt.rs` (linhas 198-245)
  - `crates/ember-core/src/project.rs` (linhas 271-283)
- **Descrição:**
  Enquanto o texto capturado do utilizador é sanitizado por `escape_input_markers` (escapando `[EMBER_INPUT]` e `[/EMBER_INPUT]`), o conteúdo de ficheiros de configuração de projetos (ex.: `CLAUDE.md`, `.cursorrules`, `README.md`) não passa por escape dos marcadores `[/EMBER_PROJECT_SOURCE]` ou `[/EMBER_PROJECT_CONTEXT]`.
- **Cenário de Risco:**
  Um utilizador que clone um repositório de terceiros contendo um `CLAUDE.md` malicioso com a string literal `[/EMBER_PROJECT_SOURCE]\nInstrução maliciosa...` pode fazer com que o LLM escape ao bloco de dados e interprete o restante texto como instrução direta ao modelo.
- **Remediação Recomendada:**
  Adicionar funções de escape para os marcadores de projeto antes de interpolar o conteúdo nas funções `build_distill_request` e `frame_project`.

---

### SEC-02: Vulnerabilidades em Dependências (Supply Chain)

#### Rust Workspace (`cargo audit`):
1. **`quick-xml` (< 0.41.0)** — RUSTSEC-2026-0194, RUSTSEC-2026-0195 (Severidade 7.5 - High)
   - *Origem:* Dependência transitiva via `plist` (ferramenta de empacotamento Tauri).
   - *Correção:* Executar `cargo update -p quick-xml`.
2. **`rkyv` (< 0.8.17)** — RUSTSEC-2026-0235
   - *Origem:* Ferramentas transitivas de compilação.
   - *Correção:* Executar `cargo update -p rkyv`.

#### Frontend (`npm audit`):
1. **`postcss` (<= 8.5.22)** — GHSA-r28c-9q8g-f849 (Severidade High)
   - Path Traversal em Source Map Auto-Loading.
   - *Correção:* `npm audit fix` ou atualizar `@tailwindcss/vite` / `vite`.
2. **`nanoid` (<= 3.3.17)** — GHSA-28wg-ghj8-5hjv (Severidade High)
   - Loop infinito em geradores customizados com tamanho negativo.
   - *Correção:* `npm audit fix`.

---

### SEC-03: Invocação do Navegador em Windows (`open_in_browser`)
- **Ficheiro afetado:** `src-tauri/src/commands.rs` (linhas 955-985)
- **Descrição:**
  A função interna `open_in_browser` executa `cmd.exe /C start "" "<url>"`. Embora recuse URLs com aspas (`url.contains('"')`) e atualmente apenas URLs estáticos pré-definidos sejam abertos, o uso de `cmd.exe` é mais sensível a caracteres especiais do que a API nativa do Windows.
- **Remediação Recomendada:**
  Utilizar diretamente a API `ShellExecuteW` da crate `windows` (`windows::Win32::UI::Shell::ShellExecuteW`) com o verbo `"open"`, eliminando a necessidade de invocar o interpretador `cmd.exe`.

---

## 4. Pontos Fortes e Boas Práticas Identificadas

1. **Isolamento de Janelas e Permissões Tauri v2:**
   - As capabilities estão estritamente segregadas por janela (`settings`, `overlay`, `picker`).
   - A janela de overlay não tem permissões de ficheiros, rede ou execução arbitrária de comandos.
2. **Content Security Policy (CSP) Rigorosa:**
   - CSP configurada em `tauri.conf.json` bloqueia injeção de scripts remotos (`default-src 'self'`).
3. **Segurança de Hooks de Teclado:**
   - O hook `WH_KEYBOARD_LL` filtra apenas as teclas essenciais (`VK_RETURN` e `VK_ESCAPE`) durante o modo preview e não faz keylogging de outras teclas.
   - Os hooks são devidamente limpos e libertados (`stop_and_join()`).
4. **Isolamento de Janelas Transparentes:**
   - O overlay e o picker usam `focus: false`, garantindo que o paste é sempre injetado na aplicação ativa do utilizador, sem risco de sequestro de foco.

---

## 5. Plano de Ação Recomendado

1. [ ] **Atualizar dependências:** Executar `cargo update -p quick-xml` e `npm audit fix`.
2. [ ] **Adicionar escape aos marcadores de contexto:** Escapar `[EMBER_PROJECT_SOURCE]` e `[EMBER_PROJECT_CONTEXT]` nas funções de framing em `crates/ember-core`.
3. [ ] **Migrar `open_in_browser` para `ShellExecuteW`:** Reduzir a superfície de ataque no Windows eliminando `cmd.exe /C start`.
