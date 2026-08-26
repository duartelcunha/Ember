//! Catalogo de modelos descoberto em runtime: guarda o que cada provider disse servir, e
//! reconcilia com o que esta gravado em disco. A decisao e pura (`ember_core::models`); aqui
//! esta so a parte com estado (cache em memoria + escrita da config).
//!
//! Nao ha pedido de rede neste modulo. A listagem chega pelo mesmo `GET /models` que ja
//! validava a chave (ver `providers::validate` -> `Probe`), por isso a descoberta nao custa
//! nem um pedido a mais nem um milissegundo no caminho do refine.

use ember_core::model::Provider;
use ember_core::models::{self, ModelInfo};
use tauri::AppHandle;

use crate::config;
use crate::state::AppState;

/// O que a UI precisa de saber sobre a listagem de um provider.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    /// Ja ordenada do melhor default para o pior candidato (`ember_core::models::rank`).
    pub models: Vec<ModelInfo>,
    /// Quando foi descoberta (epoch ms), para a UI poder dizer "lista de HH:MM" em vez de a
    /// apresentar como se fosse deste instante. `None` = nunca houve descoberta com sucesso.
    pub fetched_at_ms: Option<u64>,
    /// `false` quando estamos a servir a lista embutida no binario porque a descoberta ainda
    /// nao aconteceu (sem chave, offline, endpoint sem `/models`). A UI diz isso ao utilizador
    /// em vez de fingir que a lista e fresca. Degradar em silencio seria mentir.
    pub live: bool,
}

/// Absorve uma listagem acabada de descobrir: guarda-a no cache e reconcilia o modelo gravado.
///
/// Uma listagem VAZIA nao e tratada como "o provider nao tem modelos" mas como "nao sei": nao
/// escreve no cache nem toca na config. E a diferenca entre um modelo descontinuado (deixa de
/// aparecer numa listagem que veio bem) e uma falha de rede (nao veio listagem nenhuma).
pub fn absorb(app: &AppHandle, state: &AppState, provider: Provider, models: &[ModelInfo]) {
    if models.is_empty() {
        return;
    }
    let ranked = models::rank(provider, models);
    if let Ok(mut m) = state.model_lists.lock() {
        m.insert(provider, (ranked.clone(), crate::now_ms()));
    }
    reconcile_saved(app, provider, &ranked);
}

/// Poe o modelo gravado de acordo com o que existe hoje.
///
/// Dois comportamentos, e a diferenca esta na flag `gemini_model_auto`:
/// - **automatico** (o default): fica sempre com o melhor gratuito que o provider anunciar, e
///   acompanha sozinho as geracoes novas. E o que faz com que ninguem tenha de perceber de ids
///   de modelos para a app funcionar bem;
/// - **fixado a mao**: so mexemos se o modelo escolhido tiver DESAPARECIDO da listagem. Enquanto
///   existir, a escolha do utilizador manda, mesmo que nao fosse a nossa.
///
/// Isto e o que substitui a lista `DEAD_MODELS` escrita a mao: um modelo descontinuado desaparece
/// da listagem do provider sozinho, sem ninguem ter de o vir apagar do nosso codigo.
fn reconcile_saved(app: &AppHandle, provider: Provider, live: &[ModelInfo]) {
    let mut cfg = config::load(app);
    let d = config::Config::default();
    let (saved, fallback) = match provider {
        Provider::Gemini => (cfg.gemini_model.clone(), d.gemini_model.clone()),
        Provider::OpenAi => (cfg.openai_model.clone(), d.openai_model.clone()),
    };
    let auto = matches!(provider, Provider::Gemini) && cfg.gemini_model_auto;
    let next = if auto {
        models::pick_default(provider, live).unwrap_or(saved.clone())
    } else {
        models::reconcile(provider, &saved, live, &fallback)
    };
    if next == saved {
        return;
    }
    if auto {
        log::info!("modelo {provider:?} automatico: '{saved}' -> '{next}'");
    } else {
        log::info!("modelo {provider:?} '{saved}' ja nao existe no provider; passa a '{next}'");
    }
    match provider {
        Provider::Gemini => cfg.gemini_model = next,
        Provider::OpenAi => cfg.openai_model = next,
    }
    if let Err(e) = config::save(app, &cfg) {
        // Degrada em vez de rebentar: a app continua com o modelo antigo em disco e vai voltar
        // a tentar reconciliar no proximo probe.
        log::warn!("nao consegui gravar o modelo reconciliado de {provider:?}: {e}");
    }
}

/// Esquece a listagem de um provider. Usado quando a base URL do endpoint OpenAI-compatible
/// muda: a listagem esta COLADA ao endpoint (os modelos do Groq nao existem no OpenRouter), e
/// servir a antiga mostraria ao utilizador modelos que o novo servico nao tem. Melhor cair na
/// lista embutida, que a UI marca como nao-viva, ate o proximo probe trazer a certa.
pub fn forget(state: &AppState, provider: Provider) {
    if let Ok(mut m) = state.model_lists.lock() {
        m.remove(&provider);
    }
}

/// O catalogo a mostrar na UI para este provider. Serve o que foi descoberto; sem descoberta,
/// serve a lista embutida e diz que nao e viva (`live: false`).
pub fn catalog(state: &AppState, provider: Provider, base_url: &str) -> ModelCatalog {
    if let Ok(m) = state.model_lists.lock() {
        if let Some((models, at)) = m.get(&provider) {
            if !models.is_empty() {
                return ModelCatalog {
                    models: models.clone(),
                    fetched_at_ms: Some(*at),
                    live: true,
                };
            }
        }
    }
    ModelCatalog {
        models: fallback_catalog(provider, base_url),
        fetched_at_ms: None,
        live: false,
    }
}

/// A lista embutida no binario, usada so ate a primeira descoberta. Curta de proposito: nao e
/// para ser mantida atualizada (esse era o problema), so para o primeiro arranque, antes de
/// haver chave nenhuma, ter alguma coisa para mostrar.
fn fallback_catalog(provider: Provider, base_url: &str) -> Vec<ModelInfo> {
    use ember_core::providers as wire;
    let ids: &[&str] = match provider {
        Provider::Gemini => &[wire::DEFAULT_GEMINI_MODEL],
        Provider::OpenAi => {
            if wire::openai_is_openrouter(base_url) {
                &wire::OPENROUTER_FREE_MODELS
            } else {
                &[wire::DEFAULT_OPENAI_MODEL]
            }
        }
    };
    ids.iter()
        .map(|id| ModelInfo {
            id: (*id).to_string(),
            display_name: (*id).to_string(),
            generation: ember_core::models::parse_generation(id),
            free_tier: matches!(provider, Provider::Gemini)
                && ember_core::models::gemini_is_free_tier(id),
            preview: false,
        })
        .collect()
}
