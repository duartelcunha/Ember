//! Cache de refinamentos ja pagos.
//!
//! Existe por uma razao so: **nunca pagar duas vezes pelo mesmo texto**. Antes, qualquer
//! interrupcao (Esc, segunda tecla, uma tecla qualquer durante o preview, o timeout de 10s do
//! preview, o clipboard ocupado no paste) deitava fora uma resposta que o provider ja tinha
//! cobrado, e a reaccao natural do utilizador - carregar no atalho outra vez - pagava-a de novo.
//!
//! A chave e o texto NORMALIZADO mais tudo o que muda o resultado: o modo, o projeto ativo e uma
//! impressao digital do system prompt. Mudar o perfil ou o projeto tem de dar um miss, senao a
//! cache serve um refine feito com outras regras.
//!
//! Ha dois tipos de acerto:
//! - **exato** (apos normalizacao): sempre seguro, pode aplicar-se sem perguntar. Espacos a mais,
//!   uma quebra de linha no fim ou uma indentacao diferente nao mudam o que o modelo diria.
//! - **parecido** (>= 95% de semelhanca): NAO e seguro aplicar sozinho. Esses 5% de diferenca sao
//!   precisamente a edicao que a pessoa acabou de fazer ao texto; colar o refine antigo por cima
//!   revertia-a em silencio. Por isso so se oferece com o preview ligado, onde ela ve e aprova.

use serde::{Deserialize, Serialize};

use crate::model::RefineMode;

/// Teto de entradas guardadas. Cinquenta chega para um dia de trabalho e mantem o ficheiro
/// pequeno; a comparacao e por igualdade de string, que a esta escala nao se mede.
pub const DEFAULT_CAP: usize = 50;
/// Validade de uma entrada. Passado um dia, o texto de origem provavelmente ja mudou de
/// contexto e o perfil pode ter mudado; melhor pagar de novo do que servir algo velho.
pub const DEFAULT_TTL_MS: u64 = 24 * 60 * 60 * 1000;
/// Semelhanca minima para oferecer um acerto "parecido".
pub const SIMILAR_MIN: f64 = 0.95;

/// Normaliza o texto para efeitos de chave: tira espacos nas pontas e colapsa qualquer corrida
/// de espacos/tabs/quebras num espaco so.
///
/// So isto, de proposito. Baixar a caixa ou tirar pontuacao faria "texto diferente" parecer o
/// mesmo, e o refine de um titulo em maiusculas nao serve para o mesmo titulo em minusculas.
pub fn normalize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.trim().chars() {
        if c.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(c);
        }
    }
    out
}

/// Impressao digital estavel de uma string (FNV-1a 64).
///
/// Escrita a mao em vez de usar o `DefaultHasher` porque este nao garante o mesmo valor entre
/// execucoes: com ele, a cache em disco falhava sempre depois de reiniciar a app, que e
/// exatamente o caso que ela existe para servir.
pub fn fingerprint(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// Tudo o que, mudando, muda o refinado. Se faltasse aqui uma destas parcelas, a cache serviria
/// um resultado feito com outras regras.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    pub text: String,
    pub mode: RefineMode,
    pub project: Option<String>,
    pub prompt_fp: u64,
}

impl CacheKey {
    pub fn new(text: &str, mode: RefineMode, project: Option<&str>, system_prompt: &str) -> Self {
        Self {
            text: normalize_text(text),
            mode,
            project: project.map(str::to_owned),
            prompt_fp: fingerprint(system_prompt),
        }
    }

    /// Mesmo contexto (modo, projeto, prompt), texto a parte. Duas entradas so sao comparaveis
    /// por semelhanca se isto bater certo.
    fn same_context(&self, other: &Self) -> bool {
        self.mode == other.mode
            && self.project == other.project
            && self.prompt_fp == other.prompt_fp
    }
}

/// Um refinado ja pago, pronto a colar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// O texto POS-processado, o mesmo que teria sido colado. Guardar o cru obrigava a repetir
    /// o motor, que pode degradar, e ai a entrada em cache nao servia para nada.
    pub refined: String,
    pub provider: String,
    pub model: String,
    pub ts_ms: u64,
}

/// O que a procura encontrou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// Mesmo texto (apos normalizacao). Seguro aplicar sem perguntar.
    Exact(CacheEntry),
    /// Texto parecido. So se oferece com preview: e preciso alguem ver antes de colar.
    Similar(CacheEntry, u32),
}

/// LRU com validade. Ordem: a frente da lista e o mais recentemente usado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineCache {
    entries: Vec<(CacheKey, CacheEntry)>,
    cap: usize,
    ttl_ms: u64,
}

impl Default for RefineCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAP, DEFAULT_TTL_MS)
    }
}

impl RefineCache {
    pub fn new(cap: usize, ttl_ms: u64) -> Self {
        Self {
            entries: Vec::new(),
            cap: cap.max(1),
            ttl_ms,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Deita fora o que passou da validade.
    pub fn evict_expired(&mut self, now_ms: u64) {
        let ttl = self.ttl_ms;
        self.entries
            .retain(|(_, e)| now_ms.saturating_sub(e.ts_ms) < ttl);
    }

    /// Procura exata. Refresca a recencia da entrada encontrada: o que se usa fica.
    pub fn lookup(&mut self, key: &CacheKey, now_ms: u64) -> Option<CacheEntry> {
        self.evict_expired(now_ms);
        let idx = self.entries.iter().position(|(k, _)| k == key)?;
        let pair = self.entries.remove(idx);
        let entry = pair.1.clone();
        self.entries.insert(0, pair);
        Some(entry)
    }

    /// Procura tolerante: exato primeiro, e so depois o mais parecido acima do minimo. NAO
    /// refresca a recencia do parecido, que pode nem vir a ser usado.
    pub fn lookup_fuzzy(&mut self, key: &CacheKey, now_ms: u64, min: f64) -> Option<Hit> {
        if let Some(e) = self.lookup(key, now_ms) {
            return Some(Hit::Exact(e));
        }
        let mut best: Option<(f64, &CacheEntry)> = None;
        for (k, e) in &self.entries {
            if !k.same_context(key) {
                continue;
            }
            let score = similarity(&k.text, &key.text);
            if score >= min && best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, e));
            }
        }
        best.map(|(s, e)| Hit::Similar(e.clone(), (s * 100.0).round() as u32))
    }

    /// Guarda (ou substitui) uma entrada e mete-a a frente.
    pub fn insert(&mut self, key: CacheKey, entry: CacheEntry, now_ms: u64) {
        self.evict_expired(now_ms);
        self.entries.retain(|(k, _)| *k != key);
        self.entries.insert(0, (key, entry));
        self.entries.truncate(self.cap);
    }

    /// A entrada mais recente, seja de que texto for. E o que o "reaplicar o ultimo" cola.
    pub fn last(&self) -> Option<&CacheEntry> {
        self.entries.first().map(|(_, e)| e)
    }
}

/// Semelhanca entre dois textos, em [0,1]: 1 menos a distancia de edicao normalizada pelo
/// comprimento do maior.
///
/// Implementada aqui (Levenshtein por linhas, duas linhas de memoria) para o crate continuar sem
/// dependencias novas. Os textos que passam por aqui sao seleccoes de utilizador, na ordem dos
/// milhares de caracteres no pior caso, e so se compara contra entradas do mesmo contexto: o
/// custo e irrelevante ao pe de uma chamada HTTP a um modelo.
pub fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // Diferenca de comprimento grande de mais para chegar ao minimo: nem vale a pena a matriz.
    let longest = a.len().max(b.len()) as f64;
    if (a.len() as f64 - b.len() as f64).abs() / longest > 1.0 - SIMILAR_MIN {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    1.0 - (prev[b.len()] as f64 / longest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000;

    fn key(text: &str) -> CacheKey {
        CacheKey::new(text, RefineMode::Adaptive, None, "system v1")
    }

    fn entry(refined: &str) -> CacheEntry {
        CacheEntry {
            refined: refined.into(),
            provider: "Gemini".into(),
            model: "gemini-x".into(),
            ts_ms: NOW,
        }
    }

    #[test]
    fn whitespace_differences_are_the_same_text() {
        // Reencaminhar o mesmo paragrafo com uma quebra de linha a mais nao e texto novo, e
        // pagar por isso outra vez era o desperdicio mais banal que havia.
        let mut c = RefineCache::default();
        c.insert(key("  fix   this\n\n text  "), entry("Fixed."), NOW);
        assert_eq!(
            c.lookup(&key("fix this text"), NOW).map(|e| e.refined),
            Some("Fixed.".into())
        );
    }

    #[test]
    fn a_different_mode_or_project_or_prompt_is_a_miss() {
        // Servir um refine feito com outro perfil seria pior do que pagar de novo.
        let mut c = RefineCache::default();
        c.insert(key("hello there"), entry("Hello."), NOW);
        assert!(c
            .lookup(
                &CacheKey::new("hello there", RefineMode::Turbo, None, "system v1"),
                NOW
            )
            .is_none());
        assert!(c
            .lookup(
                &CacheKey::new("hello there", RefineMode::Adaptive, Some("p1"), "system v1"),
                NOW
            )
            .is_none());
        assert!(c
            .lookup(
                &CacheKey::new("hello there", RefineMode::Adaptive, None, "system v2"),
                NOW
            )
            .is_none());
    }

    #[test]
    fn a_near_identical_text_is_offered_as_similar_never_as_exact() {
        let mut c = RefineCache::default();
        let original = "the quick brown fox jumps over the lazy dog and keeps running";
        c.insert(key(original), entry("Refined."), NOW);
        // Uma letra diferente no meio: mesmo texto para efeitos praticos, mas nao identico.
        let edited = "the quick brown fox jumps over the lazy dig and keeps running";
        assert!(c.lookup(&key(edited), NOW).is_none(), "nunca acerto exato");
        match c.lookup_fuzzy(&key(edited), NOW, SIMILAR_MIN) {
            Some(Hit::Similar(e, score)) => {
                assert_eq!(e.refined, "Refined.");
                assert!(score >= 95, "score {score}");
            }
            other => panic!("esperava Similar, veio {other:?}"),
        }
    }

    #[test]
    fn a_text_that_really_changed_is_not_similar() {
        let mut c = RefineCache::default();
        c.insert(
            key("write a short note about the release"),
            entry("A."),
            NOW,
        );
        assert!(c
            .lookup_fuzzy(
                &key("completely different sentence with other words"),
                NOW,
                SIMILAR_MIN
            )
            .is_none());
    }

    #[test]
    fn an_exact_match_wins_over_a_similar_one() {
        let mut c = RefineCache::default();
        c.insert(key("alpha beta gamma delta epsilon"), entry("SIMILAR"), NOW);
        c.insert(key("alpha beta gamma delta epsilan"), entry("EXACT"), NOW);
        match c.lookup_fuzzy(&key("alpha beta gamma delta epsilan"), NOW, SIMILAR_MIN) {
            Some(Hit::Exact(e)) => assert_eq!(e.refined, "EXACT"),
            other => panic!("esperava Exact, veio {other:?}"),
        }
    }

    #[test]
    fn the_oldest_entry_falls_out_and_using_one_keeps_it_alive() {
        let mut c = RefineCache::new(2, DEFAULT_TTL_MS);
        c.insert(key("one"), entry("1"), NOW);
        c.insert(key("two"), entry("2"), NOW);
        // Usar a mais antiga refresca-a: a que sai a seguir e a outra.
        assert!(c.lookup(&key("one"), NOW).is_some());
        c.insert(key("three"), entry("3"), NOW);
        assert_eq!(c.len(), 2);
        assert!(c.lookup(&key("one"), NOW).is_some());
        assert!(c.lookup(&key("two"), NOW).is_none());
    }

    #[test]
    fn an_entry_past_its_validity_is_not_served() {
        let mut c = RefineCache::default();
        c.insert(key("stale"), entry("old"), NOW);
        assert!(c.lookup(&key("stale"), NOW + DEFAULT_TTL_MS - 1).is_some());
        assert!(c.lookup(&key("stale"), NOW + DEFAULT_TTL_MS).is_none());
        assert!(c.is_empty(), "a entrada expirada devia ter saido");
    }

    #[test]
    fn last_is_the_most_recent_insert() {
        let mut c = RefineCache::default();
        c.insert(key("a"), entry("A"), NOW);
        c.insert(key("b"), entry("B"), NOW);
        assert_eq!(c.last().map(|e| e.refined.as_str()), Some("B"));
    }

    #[test]
    fn last_follows_the_entry_that_was_just_reused() {
        // Reaplicar tem de colar o que foi mesmo usado por ultimo, e nao o ultimo pago.
        let mut c = RefineCache::default();
        c.insert(key("a"), entry("A"), NOW);
        c.insert(key("b"), entry("B"), NOW);
        let _ = c.lookup(&key("a"), NOW);
        assert_eq!(c.last().map(|e| e.refined.as_str()), Some("A"));
    }

    #[test]
    fn the_fingerprint_is_stable_and_separates_prompts() {
        assert_eq!(fingerprint("system v1"), fingerprint("system v1"));
        assert_ne!(fingerprint("system v1"), fingerprint("system v2"));
    }

    #[test]
    fn a_round_trip_through_json_keeps_the_entries_and_their_order() {
        // A cache so serve para o que ela existe (sobreviver a um reinicio) se isto for verdade.
        let mut c = RefineCache::default();
        c.insert(key("a"), entry("A"), NOW);
        c.insert(key("b"), entry("B"), NOW);
        let json = serde_json::to_string(&c).unwrap();
        let mut back: RefineCache = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.last().map(|e| e.refined.as_str()), Some("B"));
        assert_eq!(
            back.lookup(&key("a"), NOW).map(|e| e.refined),
            Some("A".into())
        );
    }
}
