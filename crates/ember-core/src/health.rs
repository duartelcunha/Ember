//! Prova de fallback PRE-VALIDADO e veredicto de saude dos providers. Puro, sem rede.
//!
//! A regra da casa: toda a chamada externa tem um fallback pre-validado e degrada honestamente,
//! nunca em silencio. Aqui decidimos, a partir de probes cacheados, se existe MESMO um fallback
//! conhecido-bom, e damos ao shell um veredicto para o mostrar.
//!
//! Correcao importante: a cadeia de tentativa continua a conter TODOS os providers configurados
//! (o `order_chain` so ordena/filtra pelos configurados). O probe NUNCA tira um provider da
//! cadeia, porque o `validate` bate num endpoint diferente do `refine` (GET /models vs stream
//! generate) e uma chave pode passar num e falhar no outro. O probe so informa a saude.

use crate::model::Provider;

/// TTL default de frescura de um probe (15 min). A validade de uma chave muda raramente.
pub const DEFAULT_TTL_MS: u64 = 15 * 60 * 1000;

/// Resultado do probe de validacao. Distinto de um `bool`: uma falha de rede nao diz NADA sobre
/// a chave, e colapsar os dois em `false` fazia uma chave boa parecer invalida so por offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCheck {
    Valid,
    Invalid,
    NetworkError,
}

/// Estado de um provider: configurado? e o ultimo probe (resultado + timestamp em ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderStatus {
    pub provider: Provider,
    pub configured: bool,
    pub last_check: Option<(KeyCheck, u64)>,
}

/// Veredicto de saude do sistema de refine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemHealth {
    /// Ha um fallback cross-family conhecido-bom (>= 2 providers com probe Valid fresco).
    Healthy,
    /// Funciona, mas SEM fallback pre-validado (so 1 provider, ou o 2o desconhecido/mau).
    Degraded,
    /// Nenhum provider configurado.
    Down,
}

/// O que o shell precisa de saber para reportar honestamente.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Readiness {
    pub health: SystemHealth,
    pub configured_count: usize,
    pub prevalidated_count: usize,
    /// `true` sse ha >= 2 providers provados Valid (fresco): um fallback real, pre-validado.
    pub has_prevalidated_fallback: bool,
    /// Providers cujo probe esta stale/ausente/mau e vale a pena revalidar em background.
    pub needs_revalidation: Vec<Provider>,
}

/// Avalia a saude a partir dos status por provider. So conta como "provado" um probe `Valid`
/// FRESCO (dentro do TTL). Stale/Invalid/NetworkError/ausente nao contam e vao para
/// `needs_revalidation` (o shell dispara um re-probe em background, sem bloquear o paste).
pub fn assess_providers(entries: &[ProviderStatus], now_ms: u64, ttl_ms: u64) -> Readiness {
    let configured: Vec<&ProviderStatus> = entries.iter().filter(|e| e.configured).collect();
    let configured_count = configured.len();

    let mut prevalidated = 0usize;
    let mut needs_revalidation = Vec::new();
    for e in &configured {
        match e.last_check {
            Some((KeyCheck::Valid, ts)) if now_ms.saturating_sub(ts) <= ttl_ms => prevalidated += 1,
            _ => needs_revalidation.push(e.provider),
        }
    }

    let has_prevalidated_fallback = prevalidated >= 2;
    let health = if configured_count == 0 {
        SystemHealth::Down
    } else if has_prevalidated_fallback {
        SystemHealth::Healthy
    } else {
        SystemHealth::Degraded
    };

    Readiness {
        health,
        configured_count,
        prevalidated_count: prevalidated,
        has_prevalidated_fallback,
        needs_revalidation,
    }
}

/// Ordena a cadeia de tentativa por prioridade, mantendo so os providers configurados. NUNCA
/// remove por causa de um probe: um provider configurado e sempre tentado (o probe so informa).
pub fn order_chain(configured: &[Provider], priority: &[Provider]) -> Vec<Provider> {
    priority
        .iter()
        .copied()
        .filter(|p| configured.contains(p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: u64 = 1000;

    fn st(provider: Provider, last: Option<(KeyCheck, u64)>) -> ProviderStatus {
        ProviderStatus {
            provider,
            configured: true,
            last_check: last,
        }
    }

    #[test]
    fn no_providers_is_down() {
        let r = assess_providers(&[], 5000, TTL);
        assert_eq!(r.health, SystemHealth::Down);
        assert_eq!(r.configured_count, 0);
    }

    #[test]
    fn two_fresh_valid_is_healthy_with_prevalidated_fallback() {
        let entries = [
            st(Provider::Gemini, Some((KeyCheck::Valid, 4800))),
            st(Provider::Claude, Some((KeyCheck::Valid, 4900))),
        ];
        let r = assess_providers(&entries, 5000, TTL);
        assert_eq!(r.health, SystemHealth::Healthy);
        assert!(r.has_prevalidated_fallback);
        assert_eq!(r.prevalidated_count, 2);
        assert!(r.needs_revalidation.is_empty());
    }

    #[test]
    fn single_provider_is_degraded_no_fallback() {
        let entries = [st(Provider::Gemini, Some((KeyCheck::Valid, 4900)))];
        let r = assess_providers(&entries, 5000, TTL);
        assert_eq!(r.health, SystemHealth::Degraded);
        assert!(!r.has_prevalidated_fallback);
    }

    #[test]
    fn valid_plus_network_error_is_degraded() {
        // Um Valid + um NetworkError: nao ha 2 provados, portanto Degraded (honesto).
        let entries = [
            st(Provider::Gemini, Some((KeyCheck::Valid, 4900))),
            st(Provider::Claude, Some((KeyCheck::NetworkError, 4900))),
        ];
        let r = assess_providers(&entries, 5000, TTL);
        assert_eq!(r.health, SystemHealth::Degraded);
        assert_eq!(r.prevalidated_count, 1);
        assert!(r.needs_revalidation.contains(&Provider::Claude));
    }

    #[test]
    fn fresh_invalid_primary_stays_configured_but_degrades() {
        let entries = [
            st(Provider::Gemini, Some((KeyCheck::Invalid, 4900))),
            st(Provider::Claude, Some((KeyCheck::Valid, 4900))),
        ];
        let r = assess_providers(&entries, 5000, TTL);
        assert_eq!(r.configured_count, 2); // continua configurado (nunca removido)
        assert_eq!(r.health, SystemHealth::Degraded);
        assert!(r.needs_revalidation.contains(&Provider::Gemini));
    }

    #[test]
    fn stale_valid_is_flagged_for_revalidation() {
        // Valid mas fora do TTL: nao conta como provado e pede revalidacao.
        let entries = [
            st(Provider::Gemini, Some((KeyCheck::Valid, 1000))),
            st(Provider::Claude, Some((KeyCheck::Valid, 4900))),
        ];
        let r = assess_providers(&entries, 5000, TTL);
        assert_eq!(r.prevalidated_count, 1);
        assert!(r.needs_revalidation.contains(&Provider::Gemini));
        assert_eq!(r.health, SystemHealth::Degraded);
    }

    #[test]
    fn order_chain_keeps_priority_and_filters_unconfigured() {
        let priority = [Provider::Gemini, Provider::Claude];
        assert_eq!(
            order_chain(&[Provider::Claude], &priority),
            vec![Provider::Claude]
        );
        assert_eq!(
            order_chain(&[Provider::Claude, Provider::Gemini], &priority),
            vec![Provider::Gemini, Provider::Claude]
        );
    }
}
