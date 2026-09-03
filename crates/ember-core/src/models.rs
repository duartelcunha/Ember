//! Catalogo de modelos VIVO: interpreta a listagem que cada provider publica e decide qual
//! usar. Puro e sem rede (o shell faz o GET e passa o JSON), como o resto de `ember-core`.
//!
//! Porque existe: ate aqui os ids de modelo estavam escritos a mao em quatro sitios (presets da
//! UI, defaults em `providers.rs`, e uma lista de modelos mortos em `config.rs`). Isso ja falhou
//! duas vezes em producao: o OpenRouter descontinuou o `deepseek-r1:free` que era o nosso default
//! (todo o utilizador novo apanhava erro em TODOS os refines), e chegou a ir para disco um
//! `gemini-3.5-flash` que nunca existiu. Uma lista escrita a mao envelhece sozinha; a listagem do
//! provider nao. Aqui derivamos em vez de enumerar.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::Provider;

/// Um modelo utilizavel para refinar, normalizado a partir do formato de cada provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// O id que vai no pedido (ex: "gemini-2.5-flash", "meta-llama/llama-3.3-70b-instruct:free").
    pub id: String,
    /// Nome legivel para a UI. Cai para o `id` quando o provider nao da nenhum.
    pub display_name: String,
    /// Geracao extraida do id, para ordenar do mais recente para o mais antigo. Ver
    /// `parse_generation`. `0` = nao foi possivel determinar.
    pub generation: u32,
    /// Plausivelmente no free tier. Facto quando o provider o diz (sufixo `:free` do
    /// OpenRouter); heuristica de nome no Gemini, onde a API nao publica esta informacao.
    pub free_tier: bool,
    /// Preview/experimental. Servem para escolher a mao, nunca para o default automatico:
    /// desaparecem sem aviso, que e exatamente o problema que este modulo resolve.
    pub preview: bool,
}

impl ModelInfo {
    fn new(id: impl Into<String>, display_name: Option<String>, free_tier: bool) -> Self {
        let id = id.into();
        let display_name = display_name
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        Self {
            generation: parse_generation(&id),
            preview: is_preview(&id),
            free_tier,
            display_name,
            id,
        }
    }
}

/// Marcas de um modelo instavel. `-latest` fica DE FORA de proposito: e um alias estavel para o
/// mais recente da familia, nao um preview.
const PREVIEW_MARKERS: [&str; 4] = ["-preview", "-exp", "-experimental", "-rc"];

fn is_preview(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    PREVIEW_MARKERS.iter().any(|m| lower.contains(m))
}

/// Familias que nao geram texto e nunca servem para refinar, mesmo que a listagem as devolva.
const NON_TEXT_MARKERS: [&str; 7] = [
    "embedding",
    "embed-",
    "aqa",
    "imagen",
    "veo",
    "-tts",
    "whisper",
];

fn is_non_text(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    NON_TEXT_MARKERS.iter().any(|m| lower.contains(m))
}

/// Modelos ESPECIALIZADOS que a listagem do Gemini devolve com `generateContent` mas que nao
/// servem para refinar um prompt: musica (lyria), imagem (nano-banana, `-image-`), robotica,
/// controlo de computador, pesquisa profunda, audio nativo e as sessoes `live`.
///
/// Porque isto e preciso alem do `NON_TEXT_MARKERS`: eles ANUNCIAM `generateContent`, portanto
/// o filtro de capacidade deixa-os passar. A listagem real trazia 34 modelos e a maioria era
/// isto, o que fazia da escolha do modelo uma lista impossivel de ler. Anunciar uma capacidade
/// nao e o mesmo que servir para o trabalho.
const SPECIALTY_MARKERS: [&str; 9] = [
    "lyria",
    "nano-banana",
    "-image",
    "image-",
    "robotics",
    "computer-use",
    "deep-research",
    "native-audio",
    "-live-",
];

/// Este id do Gemini serve para refinar texto? Exige ser da familia `gemini-` (o resto da
/// listagem sao produtos a parte) e nao ser um dos especializados.
pub fn gemini_is_text_refiner(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.starts_with("gemini-")
        && !is_non_text(&lower)
        && !SPECIALTY_MARKERS.iter().any(|m| lower.contains(m))
}

/// Extrai a geracao de um id de modelo como `major * 100 + minor`, para "2.5" < "3.1" < "4.5"
/// ordenarem como numeros e nao como strings (em ordem lexicografica "10" vinha antes de "9").
///
/// As regras sao apertadas de proposito, porque os ids misturam versoes com outros numeros:
/// - o major tem de ter 1 ou 2 digitos, senao apanhavamos contagens de parametros ("gpt-oss-120b")
///   e datas ("claude-3-opus-20240229");
/// - um minor separado por "." aceita 1 ou 2 digitos ("llama-3.3" da 303, "gemini-3.1" da 301);
/// - um minor separado por "-" aceita SO 1 digito, para apanhar "claude-haiku-4-5" (405) sem
///   apanhar "gpt-4-32k" (que daria 432 e passaria a frente do gpt-4.1).
///
/// Um id sem nada reconhecivel da `0` e fica no fim da ordenacao, em vez de rebentar.
pub fn parse_generation(id: &str) -> u32 {
    let b = id.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let digits = &id[start..i];
        // Mais de 2 digitos: e uma data ou uma contagem de parametros, nao uma versao. Salta e
        // continua a procurar mais a frente no id.
        if digits.len() > 2 {
            continue;
        }
        let major: u32 = digits.parse().unwrap_or(0);
        let minor = match b.get(i) {
            Some(b'.') => take_minor(id, i + 1, 2),
            Some(b'-') => take_minor(id, i + 1, 1),
            _ => None,
        };
        return major * 100 + minor.unwrap_or(0);
    }
    0
}

/// Le ate `max_digits` digitos a partir de `from`. Rejeita a corrida inteira se for mais longa
/// do que o permitido, em vez de a truncar: truncar "32" para "3" em "gpt-4-32k" daria 403, que
/// e pior do que os 400 honestos de nao saber o minor.
fn take_minor(id: &str, from: usize, max_digits: usize) -> Option<u32> {
    let b = id.as_bytes();
    let mut j = from;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    let n = j - from;
    if n == 0 || n > max_digits {
        return None;
    }
    id[from..j].parse().ok()
}

// ---------------------------------------------------------------------------------------------
// Interpretacao da listagem de cada provider
// ---------------------------------------------------------------------------------------------

/// `GET https://generativelanguage.googleapis.com/v1beta/models`. Fica so com o que sabe fazer
/// `generateContent`: a mesma listagem traz embeddings, imagem e video, que dariam 400 aqui.
///
/// O free tier NAO vem na resposta (a Google publica-o so na pagina de pricing), por isso e uma
/// heuristica de nome sobre a familia flash. E uma linha a corrigir se a Google mudar as
/// fronteiras, em vez das quatro listas escritas a mao que isto substitui.
pub fn parse_gemini_models(body: &Value) -> Vec<ModelInfo> {
    let Some(arr) = body.get("models").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter(|m| {
            m.get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .is_some_and(|ms| ms.iter().any(|v| v.as_str() == Some("generateContent")))
        })
        .filter_map(|m| {
            // `name` vem prefixado: "models/gemini-2.5-flash".
            let id = m
                .get("name")
                .and_then(Value::as_str)?
                .trim_start_matches("models/")
                .to_string();
            if !gemini_is_text_refiner(&id) {
                return None;
            }
            let free = gemini_is_free_tier(&id);
            let display = m
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(ModelInfo::new(id, display, free))
        })
        .collect()
}

/// Heuristica de free tier do Gemini: a familia flash (incluindo flash-lite) tem quota gratuita,
/// pro e ultra nao. Ver a nota em `parse_gemini_models` sobre porque isto e um palpite informado
/// e nao um facto da API.
pub fn gemini_is_free_tier(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.contains("flash") && !lower.contains("pro") && !lower.contains("ultra")
}

/// `GET {base_url}/models` de qualquer endpoint OpenAI-compatible (Groq, OpenAI, OpenRouter,
/// DeepSeek, Ollama), resposta minima comum `{"data":[{"id"}]}`. O OpenRouter marca os modelos
/// gratuitos no proprio id, com o sufixo `:free`, e ai o free tier e facto e nao heuristica.
pub fn parse_openai_models(body: &Value) -> Vec<ModelInfo> {
    parse_data_array(body, |m| {
        // `name` e o campo do OpenRouter; `display_name` e o da Anthropic, que entra por aqui
        // desde que deixou de ter caminho proprio. Sem o segundo, um modelo Claude aparecia na
        // lista com o id cru em vez do nome legivel.
        let display = m
            .get("name")
            .or_else(|| m.get("display_name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let free = m
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.ends_with(":free"));
        (display, free)
    })
}

/// Troco comum aos dois formatos `{"data":[{"id",...}]}`.
fn parse_data_array(
    body: &Value,
    extract: impl Fn(&Value) -> (Option<String>, bool),
) -> Vec<ModelInfo> {
    let Some(arr) = body.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?.to_string();
            if id.is_empty() || is_non_text(&id) {
                return None;
            }
            let (display, free) = extract(m);
            Some(ModelInfo::new(id, display, free))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Ordenacao e escolha do default
// ---------------------------------------------------------------------------------------------

/// Family weight within a provider: on Gemini, which family actually answers on the free tier.
///
/// This order used to be reversed, on the grounds that `flash` and `flash-lite` shared "the same
/// free quota" and flash had the better quality. The premise was false. In real use the current
/// flash generation returns `503 UNAVAILABLE - high demand` back to back (in a single day, 3 of 3
/// on `gemini-3.8-flash` and 2 of 3 on `3.7-flash`) while the lite family answers. Google no
/// longer publishes per-model quotas, but the observed behaviour is enough: the free flagship is
/// the most contended one, and a better model that never answers is worth nothing to a refine
/// that has to land in seconds.
///
/// Hence `flash-lite` > `flash` > `pro`. `pro` is usually not free anyway, and in that case it
/// already drops out on the first `sort_key` criterion.
fn family_rank(provider: Provider, id: &str) -> u8 {
    let l = id.to_ascii_lowercase();
    match provider {
        Provider::Gemini => {
            if l.contains("flash-lite") {
                3
            } else if l.contains("flash") {
                2
            } else if l.contains("pro") {
                1
            } else {
                0
            }
        }
        // Endpoint arbitrario (Groq, OpenAI, Anthropic, DeepSeek, Ollama): nao ha familias
        // que saibamos comparar entre si sem inventar uma tabela por vendedor, que envelhecia
        // como as listas que este modulo veio substituir. Tudo empata aqui e o desempate fica
        // com a geracao. Na pratica pesa pouco: o `reconcile` so escolhe sozinho quando o
        // modelo gravado desapareceu, e mesmo ai tenta primeiro o default do endpoint.
        Provider::OpenAi => 0,
    }
}

/// Sort key, best to worst. The criteria, in order, and why:
/// 1. free tier first (the explicit requirement: keep Gemini on free models);
/// 2. stable before preview (a preview disappears without notice);
/// 3. the family with real capacity (see `family_rank`);
/// 4. newest generation, WITHIN that family.
///
/// Family now outranks generation, and that is the change that matters: with generation in
/// charge, the pick was always the freshest flash, which is precisely the most contended model on
/// the free tier. Generation still decides inside a family so we do not end up clinging to old
/// models that Google retires (a discontinued flash-lite already returned a 404 on us).
fn sort_key(provider: Provider, m: &ModelInfo) -> (bool, bool, u8, u32) {
    (
        m.free_tier,
        !m.preview,
        family_rank(provider, &m.id),
        m.generation,
    )
}

/// Ordena do melhor para o pior candidato a default. Nao remove nada: o utilizador continua a
/// poder escolher a mao um `pro` ou um preview na UI, so nao e isso que apanha sozinho.
pub fn rank(provider: Provider, models: &[ModelInfo]) -> Vec<ModelInfo> {
    let mut out = models.to_vec();
    out.sort_by(|a, b| {
        sort_key(provider, b)
            .cmp(&sort_key(provider, a))
            // Desempate final estavel pelo id, para a lista nao dancar entre refrescos.
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// O melhor default automatico para este provider, ou `None` se a listagem estiver vazia.
pub fn pick_default(provider: Provider, models: &[ModelInfo]) -> Option<String> {
    rank(provider, models).into_iter().next().map(|m| m.id)
}

/// Modelos da MESMA familia a tentar quando o escolhido falha por uma razao que e do MODELO e
/// nao do provider: sobrecarga (503 "high demand", 429) ou um id que deixou de existir (404).
///
/// Porque existe: o `gemini-3.7-flash` e o mais recente e por isso o mais concorrido do free
/// tier. Um utilizador com a chave perfeita apanhava 503 atras de 503 nesse modelo, e como a
/// cadeia so tinha um passo por familia o Ember dava o Gemini inteiro por perdido e saltava para
/// o fallback, apesar de a mesma chave servir mais quinze modelos, cada um com a sua fila.
///
/// Duas regras, e as duas sao conservadoras de proposito:
/// - **so modelos gratuitos, e so quando o escolhido tambem e gratuito**. Um alternativo pago
///   gastaria dinheiro num modelo que o utilizador nao pediu, que e a mesma linha que o
///   `openai_fallback_models` ja recusa atravessar;
/// - **nunca previews**: uma rede de seguranca tem de ser a parte aborrecida e estavel do
///   catalogo, nao a que desaparece sem aviso.
///
/// Catalogo vazio (offline, sem chave, endpoint sem `/models`) devolve vazio: sem listagem nao
/// sabemos que modelos existem, e inventar ids seria voltar exatamente ao problema que este
/// modulo resolve.
pub fn alternates(
    provider: Provider,
    chosen: &str,
    catalog: &[ModelInfo],
    max: usize,
) -> Vec<String> {
    let chosen_is_free = catalog
        .iter()
        .find(|m| m.id == chosen)
        .is_some_and(|m| m.free_tier);
    if !chosen_is_free || max == 0 {
        return Vec::new();
    }
    rank(provider, catalog)
        .into_iter()
        .filter(|m| m.id != chosen && m.free_tier && !m.preview)
        .map(|m| m.id)
        .take(max)
        .collect()
}

/// Reconcilia o modelo gravado em disco com o que o provider diz existir HOJE.
///
/// Substitui a lista `DEAD_MODELS` escrita a mao: um modelo descontinuado deixa simplesmente de
/// aparecer na listagem, e isso e facto e nao palpite. Regras, por ordem:
/// 1. listagem vazia (offline, sem chave, endpoint que nao serve `/models`) -> **nao toca em
///    nada**. Nao sabemos nada de novo, e trocar o modelo do utilizador por causa de uma falha de
///    rede seria mentir-lhe;
/// 2. o modelo gravado ainda existe -> fica, mesmo que nao fosse o que escolhiamos hoje. A
///    escolha e dele;
/// 3. deixou de existir -> o `fallback`, se esse existir; senao o melhor da listagem.
pub fn reconcile(provider: Provider, saved: &str, live: &[ModelInfo], fallback: &str) -> String {
    if live.is_empty() {
        return saved.to_string();
    }
    let exists = |id: &str| !id.is_empty() && live.iter().any(|m| m.id == id);
    if exists(saved) {
        return saved.to_string();
    }
    if exists(fallback) {
        return fallback.to_string();
    }
    pick_default(provider, live).unwrap_or_else(|| saved.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(id: &str, free: bool) -> ModelInfo {
        ModelInfo::new(id, None, free)
    }

    /// A forma serializada E o contrato com o `ModelInfo` do TS. Um campo em snake_case
    /// chegaria `undefined` ao lado do webview sem erro nenhum, e a UI mostrava a lista errada
    /// em silencio. Pina os nomes aqui, como o `provider_serializes_to_ipc_ids` faz no `model`.
    #[test]
    fn model_info_serializes_to_camel_case_for_the_ui() {
        let json = serde_json::to_string(&m("gemini-2.5-flash", true)).unwrap();
        assert!(json.contains("\"displayName\""), "{json}");
        assert!(json.contains("\"freeTier\""), "{json}");
        assert!(!json.contains("display_name"), "{json}");
        assert!(!json.contains("free_tier"), "{json}");
    }

    #[test]
    fn generation_orders_versions_as_numbers_not_strings() {
        assert_eq!(parse_generation("gemini-2.5-flash"), 205);
        assert_eq!(parse_generation("gemini-3.1-flash-lite"), 301);
        assert!(parse_generation("gemini-10.0-flash") > parse_generation("gemini-9.0-flash"));
        // Minor separado por "-": o formato dos ids recentes da Anthropic.
        assert_eq!(parse_generation("claude-haiku-4-5"), 405);
        assert_eq!(parse_generation("claude-sonnet-4-6"), 406);
    }

    #[test]
    fn generation_ignores_dates_and_parameter_counts() {
        // A data no fim nao e versao: conta o "3" e para.
        assert_eq!(parse_generation("claude-3-opus-20240229"), 300);
        // "120b" sao parametros, nao versao. Sem outro numero no id, fica desconhecido.
        assert_eq!(parse_generation("openai/gpt-oss-120b"), 0);
        // O "70b" e ignorado; a versao e o 3.3 que vem antes.
        assert_eq!(parse_generation("llama-3.3-70b-versatile"), 303);
        // Minor de 2 digitos depois de "-" e recusado: "gpt-4-32k" e 400, nao 432, senao
        // passava a frente do gpt-4.1 (401), que e mais recente.
        assert_eq!(parse_generation("gpt-4-32k"), 400);
        assert_eq!(parse_generation("gpt-4.1-mini"), 401);
        assert!(parse_generation("gpt-4.1-mini") > parse_generation("gpt-4-32k"));
        // Nada reconhecivel devolve 0 em vez de rebentar.
        assert_eq!(parse_generation("some-custom-local-model"), 0);
        assert_eq!(parse_generation(""), 0);
    }

    #[test]
    fn gemini_listing_keeps_only_text_generation_models() {
        let body = json!({"models": [
            {"name": "models/gemini-2.5-flash", "displayName": "Gemini 2.5 Flash",
             "supportedGenerationMethods": ["generateContent", "countTokens"]},
            // Embeddings: sabe fazer embedContent, nao generateContent. Fora.
            {"name": "models/text-embedding-004", "displayName": "Embedding",
             "supportedGenerationMethods": ["embedContent"]},
            // Imagen anuncia predict, e alem disso cai no filtro de familia nao-textual.
            {"name": "models/imagen-4.0-generate", "supportedGenerationMethods": ["predict"]},
        ]});
        let got = parse_gemini_models(&body);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "gemini-2.5-flash");
        // O prefixo "models/" nao pode ir para o pedido.
        assert!(!got[0].id.contains('/'));
        assert_eq!(got[0].display_name, "Gemini 2.5 Flash");
        assert!(got[0].free_tier);
    }

    #[test]
    fn the_gemini_listing_drops_everything_that_is_not_a_text_refiner() {
        // Ids reais vindos da listagem do Gemini, tal como apareceram no dropdown: 34 modelos,
        // e a maioria deles nada tinha a ver com refinar texto. Todos ANUNCIAM generateContent,
        // por isso o filtro de capacidade sozinho deixava-os passar.
        let lixo = [
            "lyria-3-pro-preview", // musica
            "lyria-3-clip-preview",
            "nano-banana-pro-preview", // imagem
            "gemini-3-pro-image-preview",
            "gemini-robotics-er-2-preview", // robotica
            "gemini-2.5-computer-use-preview-10-2025",
            "deep-research-pro-preview-12-2025",
            "antigravity-preview-05-2026",
            "gemini-2.5-flash-native-audio",
        ];
        for id in lixo {
            assert!(!gemini_is_text_refiner(id), "{id} devia ter sido filtrado");
        }
        // E o que serve continua a passar.
        for id in [
            "gemini-2.5-flash",
            "gemini-3.1-flash-lite",
            "gemini-3.5-flash",
            "gemini-2.5-pro",
            "gemini-pro-latest",
        ] {
            assert!(gemini_is_text_refiner(id), "{id} devia ter passado");
        }
    }

    #[test]
    fn gemini_free_tier_is_the_flash_family_only() {
        assert!(gemini_is_free_tier("gemini-2.5-flash"));
        assert!(gemini_is_free_tier("gemini-3.1-flash-lite"));
        assert!(!gemini_is_free_tier("gemini-2.5-pro"));
        assert!(!gemini_is_free_tier("gemini-3.0-ultra"));
    }

    #[test]
    fn model_listings_parse_the_shapes_the_providers_actually_send() {
        // A Anthropic entra pelo caminho OpenAI-compativel e usa `display_name` onde o
        // OpenRouter usa `name`. Os dois tem de ser lidos, senao um modelo Claude aparecia na
        // lista com o id cru.
        let anthropic = json!({"data": [
            {"id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5", "type": "model"},
        ]});
        let got = parse_openai_models(&anthropic);
        assert_eq!(got[0].id, "claude-haiku-4-5");
        assert_eq!(got[0].display_name, "Claude Haiku 4.5");

        let openrouter = json!({"data": [
            {"id": "meta-llama/llama-3.3-70b-instruct:free", "name": "Llama 3.3 70B (free)"},
            {"id": "anthropic/claude-sonnet-4-6"},
        ]});
        let got = parse_openai_models(&openrouter);
        // O sufixo ":free" e facto publicado pelo OpenRouter, nao heuristica nossa.
        assert!(got[0].free_tier);
        assert!(!got[1].free_tier);
        // Sem "name", o display cai para o id em vez de ficar vazio na UI.
        assert_eq!(got[1].display_name, "anthropic/claude-sonnet-4-6");
    }

    #[test]
    fn malformed_listings_yield_nothing_instead_of_panicking() {
        // Um provider que muda o formato, ou um erro devolvido com 200: lista vazia, e o
        // `reconcile` trata isso como "nao sei nada" e nao mexe na escolha do utilizador.
        assert!(parse_gemini_models(&json!({})).is_empty());
        assert!(parse_gemini_models(&json!({"models": "nope"})).is_empty());
        assert!(parse_openai_models(&json!({"data": [{"no_id": 1}]})).is_empty());
        assert!(parse_openai_models(&json!({"error": "unauthorized"})).is_empty());
    }

    #[test]
    fn ranking_prefers_free_stable_and_recent_in_that_order() {
        let models = vec![
            m("gemini-2.5-flash", true),
            m("gemini-3.1-flash-lite", true),
            m("gemini-3.5-flash", true),
            m("gemini-3.5-flash-preview-01-01", true),
            m("gemini-3.5-pro", false),
        ];
        let ranked = rank(Provider::Gemini, &models);
        let ids: Vec<&str> = ranked.iter().map(|x| x.id.as_str()).collect();
        // `pro` is more capable but not free tier: it leaves the top without leaving the list.
        // And the top is the lite, even one generation behind: that is the family that actually
        // answers on the free tier.
        assert_eq!(ids[0], "gemini-3.1-flash-lite");
        assert_eq!(*ids.last().unwrap(), "gemini-3.5-pro");
        // Entre dois free da mesma geracao, o estavel ganha ao preview.
        assert!(
            ids.iter().position(|i| *i == "gemini-3.5-flash")
                < ids
                    .iter()
                    .position(|i| *i == "gemini-3.5-flash-preview-01-01")
        );
        // Newer generation beats older WITHIN the same family.
        assert!(
            ids.iter().position(|i| *i == "gemini-3.5-flash")
                < ids.iter().position(|i| *i == "gemini-2.5-flash")
        );
    }

    #[test]
    fn ranking_prefers_flash_lite_because_the_flash_free_tier_has_no_capacity() {
        // This rule used to be the other way round ("flash over lite: same quota, better
        // quality"). Real usage killed the premise: in a single day `gemini-3.8-flash` returned
        // 503 "high demand" on 3 of 3 attempts and `3.7-flash` on 2 of 3, while the lite family
        // answered. A better model that never answers is worth less than a decent one that
        // always does.
        let models = vec![
            m("gemini-3.5-flash-lite", true),
            m("gemini-3.5-flash", true),
        ];
        assert_eq!(
            pick_default(Provider::Gemini, &models).unwrap(),
            "gemini-3.5-flash-lite"
        );
    }

    #[test]
    fn a_lite_of_an_older_generation_beats_a_brand_new_flash() {
        // Family now weighs MORE than generation: it was generation being in charge that put
        // `3.8-flash` on top, which is precisely the most contended model of them all.
        let models = vec![
            m("gemini-3.8-flash", true),
            m("gemini-3.5-flash-lite", true),
        ];
        assert_eq!(
            pick_default(Provider::Gemini, &models).unwrap(),
            "gemini-3.5-flash-lite"
        );
    }

    #[test]
    fn between_two_lites_the_newer_generation_still_wins() {
        // Generation still decides WITHIN the family: preferring capacity is not preferring old
        // models, which end up retired (a 2.5 flash-lite already returned a 404 on us).
        let models = vec![
            m("gemini-3.5-flash-lite", true),
            m("gemini-3.8-flash-lite", true),
        ];
        assert_eq!(
            pick_default(Provider::Gemini, &models).unwrap(),
            "gemini-3.8-flash-lite"
        );
    }

    #[test]
    fn ranking_is_stable_across_refreshes() {
        // Dois modelos indistinguiveis pela chave: o desempate pelo id impede a lista de dancar
        // na UI de refresco para refresco.
        let a = vec![m("zeta-1.0", true), m("alpha-1.0", true)];
        let b = vec![m("alpha-1.0", true), m("zeta-1.0", true)];
        assert_eq!(rank(Provider::OpenAi, &a), rank(Provider::OpenAi, &b));
    }

    #[test]
    fn reconcile_keeps_the_users_model_when_it_still_exists() {
        let live = vec![m("gemini-2.5-flash", true), m("gemini-3.5-flash", true)];
        // Nao e o que escolhiamos hoje, mas a escolha e dele.
        assert_eq!(
            reconcile(
                Provider::Gemini,
                "gemini-2.5-flash",
                &live,
                "gemini-3.5-flash"
            ),
            "gemini-2.5-flash"
        );
    }

    #[test]
    fn reconcile_replaces_a_model_that_no_longer_exists() {
        // O caso `deepseek-r1:free`: descontinuado pelo provider, e todo o refine dava erro.
        let live = vec![m("gemini-3.5-flash", true)];
        assert_eq!(
            reconcile(
                Provider::Gemini,
                "gemini-1.0-pro",
                &live,
                "gemini-3.5-flash"
            ),
            "gemini-3.5-flash"
        );
        // Fallback tambem morto: cai no melhor da listagem viva, nunca num id inventado.
        assert_eq!(
            reconcile(Provider::Gemini, "gemini-1.0-pro", &live, "tambem-morto"),
            "gemini-3.5-flash"
        );
    }

    #[test]
    fn reconcile_does_not_touch_anything_when_discovery_failed() {
        // Offline, sem chave, ou endpoint sem `/models`: lista vazia nao e prova de que o modelo
        // morreu. Trocar aqui seria mudar a config do utilizador por causa de uma falha de rede.
        assert_eq!(
            reconcile(
                Provider::Gemini,
                "gemini-2.5-flash",
                &[],
                "gemini-3.5-flash"
            ),
            "gemini-2.5-flash"
        );
    }

    /// Catalogo parecido com o que a chave do Google AI Studio devolve mesmo.
    fn gemini_catalog() -> Vec<ModelInfo> {
        vec![
            m("gemini-3.7-flash", true),
            m("gemini-3.6-flash", true),
            m("gemini-3.5-flash-lite", true),
            m("gemini-2.5-flash", true),
            m("gemini-3-flash-preview", true),
            m("gemini-2.5-pro", false),
        ]
    }

    #[test]
    fn alternates_are_the_next_best_free_models_of_the_same_family() {
        // O caso que motivou isto: o 3.7-flash e o mais recente e por isso o mais concorrido,
        // e devolvia 503 "high demand" em serie. Os outros flash da mesma chave estavam livres.
        // And the safety net starts with the LITE: falling from the most contended model to the
        // second most contended one is what the log showed us doing, 503 after 503.
        let alt = alternates(Provider::Gemini, "gemini-3.7-flash", &gemini_catalog(), 2);
        assert_eq!(alt, vec!["gemini-3.5-flash-lite", "gemini-3.6-flash"]);
        // O escolhido nunca se repete a si proprio (seria gastar um pedido para apanhar o mesmo
        // 503), e o preview fica de fora (uma rede de seguranca nao pode desaparecer sem aviso).
        assert!(!alt.contains(&"gemini-3.7-flash".to_string()));
        assert!(!alt.contains(&"gemini-3-flash-preview".to_string()));
    }

    #[test]
    fn alternates_never_spend_money_the_user_did_not_choose() {
        // Quem escolheu um modelo pago pediu AQUELE modelo. Trocar-lho por outro pelas costas
        // gastava-lhe dinheiro e mudava-lhe a qualidade sem ele saber, que e a mesma linha que o
        // `openai_fallback_models` ja recusa atravessar para o OpenRouter.
        assert!(alternates(Provider::Gemini, "gemini-2.5-pro", &gemini_catalog(), 2).is_empty());
        // E um alternativo pago nunca entra, mesmo com o escolhido gratuito.
        let alt = alternates(Provider::Gemini, "gemini-3.7-flash", &gemini_catalog(), 9);
        assert!(!alt.contains(&"gemini-2.5-pro".to_string()));
    }

    #[test]
    fn alternates_are_empty_without_a_live_catalog() {
        // Sem listagem nao sabemos que modelos existem. Inventar ids aqui era voltar ao problema
        // que este modulo inteiro existe para resolver.
        assert!(alternates(Provider::Gemini, "gemini-3.7-flash", &[], 2).is_empty());
        // Um modelo escrito a mao que nao esta no catalogo tambem nao gera alternativos: nao
        // sabemos se e gratuito, e assumir que sim podia custar dinheiro ao utilizador.
        let c = gemini_catalog();
        assert!(alternates(Provider::Gemini, "gemini-9.9-flash", &c, 2).is_empty());
        assert!(alternates(Provider::Gemini, "gemini-3.7-flash", &c, 0).is_empty());
    }
}
