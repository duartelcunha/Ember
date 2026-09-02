//! Guardas fail-safe: so DECIDEM recusar colar, nunca reescrevem. Um Polish quase-identico e
//! sucesso, nao falha (nao ha guarda de "no-op"). Puro.

use super::mask::SpanTable;

/// `true` se o texto e efetivamente vazio (so espacos/quebras). Depois de limpar, um output
/// assim nunca deve ser colado por cima da seleccao.
pub fn is_effectively_empty(s: &str) -> bool {
    s.trim().is_empty()
}

/// `true` se TODOS os tokens de span continuam presentes no output do modelo. Um token em falta
/// significa que o modelo deitou fora (ou mutou) um pedaco de codigo/URL: nao da para restaurar
/// intacto, por isso degrada em vez de colar codigo partido.
pub fn check_preservation(table: &SpanTable, output: &str) -> bool {
    table.tokens().all(|t| output.contains(t))
}

// -------------------------------------------------------------------------------------------
// Guarda de lingua
// -------------------------------------------------------------------------------------------
//
// O system prompt diz tres vezes para nunca traduzir, e mesmo assim ja aconteceu: um CLAUDE.md
// em portugues com "responder na lingua do utilizador" fez um input INGLES sair traduzido. O
// prompt e uma instrucao, nao uma garantia; isto e a garantia. O teste
// `prompt::tests::language_preservation_beats_the_profile` anotava esta guarda como trabalho
// futuro, e e este o trabalho.
//
// Desenho conservador, porque um falso positivo aqui recusa um refine BOM: so acusa quando as
// DUAS pontas sao identificadas com folga e discordam. Portugues confundido com espanhol nao da
// veredicto, da empate, e um empate deixa passar. Preferimos deixar escapar uma traducao a
// recusar trabalho legitimo, porque o custo de cada lado nao e o mesmo: uma traducao o
// utilizador ve e desfaz, uma recusa injustificada faz a app parecer partida.

/// Minimo de palavras em cada ponta para haver veredicto. Abaixo disto nao ha evidencia: uma
/// frase de cinco palavras nao chega para dizer em que lingua esta.
const MIN_WORDS_FOR_VERDICT: usize = 15;
/// Minimo de acertos da lingua vencedora para haver veredicto nenhum.
const MIN_HITS: usize = 4;

/// Palavras funcionais por lingua. Escolhidas por serem frequentes E discriminantes; algumas
/// repetem-se entre linguas proximas de proposito (o "que" em pt/es/fr), e e isso que faz o
/// desempate falhar entre elas em vez de escolher a sorte.
const STOPWORDS: [(&str, &[&str]); 6] = [
    (
        "en",
        &[
            "the", "and", "of", "to", "is", "that", "for", "with", "you", "this", "are", "not",
            "your", "it",
        ],
    ),
    (
        "pt",
        &[
            "que", "nao", "uma", "com", "para", "como", "mais", "voce", "esta", "isso", "tambem",
            "muito", "quando", "porque", "dos", "das",
        ],
    ),
    (
        "es",
        &[
            "que", "una", "con", "para", "como", "mas", "usted", "esto", "tambien", "muy",
            "cuando", "porque", "pero", "los", "las", "sus",
        ],
    ),
    (
        "fr",
        &[
            "que", "une", "avec", "pour", "comme", "plus", "vous", "est", "aussi", "tres", "quand",
            "parce", "alors", "les", "des", "dans",
        ],
    ),
    (
        "de",
        &[
            "und", "der", "die", "das", "nicht", "ein", "eine", "mit", "fur", "wie", "sie", "ist",
            "auch", "sehr", "wenn", "weil",
        ],
    ),
    (
        "it",
        &[
            "che", "non", "una", "con", "per", "come", "piu", "questo", "anche", "molto", "quando",
            "perche", "gli", "dei", "nel", "sono",
        ],
    ),
];

/// Grupo de escrita dominante. Nao ha grafia partilhada entre estes: um input em Han a sair em
/// Latin e uma traducao, sem margem para duvida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Cyrillic,
    Greek,
    Han,
    Kana,
    Hangul,
    Arabic,
    Hebrew,
    Other,
}

fn script_of(c: char) -> Script {
    match c as u32 {
        0x0041..=0x024F => Script::Latin,
        0x0370..=0x03FF => Script::Greek,
        0x0400..=0x04FF => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF => Script::Arabic,
        0x3040..=0x30FF => Script::Kana,
        0x4E00..=0x9FFF => Script::Han,
        0xAC00..=0xD7AF => Script::Hangul,
        _ => Script::Other,
    }
}

/// A escrita dominante de um texto, se alguma passar dos 60% das letras. Menos do que isso e
/// texto misto (codigo com comentarios, citacoes), e ai nao ha veredicto.
fn dominant_script(text: &str) -> Option<Script> {
    let mut counts = [0usize; 9];
    let idx = |s: Script| s as usize;
    let mut total = 0usize;
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        let s = script_of(c);
        if s == Script::Other {
            continue;
        }
        counts[idx(s)] += 1;
        total += 1;
    }
    if total < 10 {
        return None;
    }
    let all = [
        Script::Latin,
        Script::Cyrillic,
        Script::Greek,
        Script::Han,
        Script::Kana,
        Script::Hangul,
        Script::Arabic,
        Script::Hebrew,
    ];
    all.into_iter()
        .max_by_key(|s| counts[idx(*s)])
        .filter(|s| counts[idx(*s)] * 10 >= total * 6)
}

/// Reduz o texto a palavras minusculas SEM acentos, para "nao"/"não" e "mais"/"máis" contarem
/// pela mesma entrada. Sem isto, a lista teria de duplicar cada palavra acentuada, e um input
/// escrito sem acentos (comum quando se escreve depressa) deixava de ser reconhecido.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(|w| {
            w.chars()
                .map(deaccent)
                .flat_map(char::to_lowercase)
                .collect()
        })
        .collect()
}

fn deaccent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        other => other,
    }
}

/// As linguas PLAUSIVEIS para um texto: a melhor e todas as que ficam a menos de metade dela.
/// Vazio quando nao ha evidencia que chegue.
///
/// Devolve um conjunto e nao uma lingua unica de proposito. Perguntar "que lingua e esta?" nao
/// tem resposta fiavel com listas de palavras funcionais: portugues e espanhol partilham "que",
/// "para", "porque", e um texto portugues pontua alto em espanhol. Mas a pergunta que nos
/// interessa nao e essa, e sim "as duas pontas podem ser a mesma lingua?". Para isso um conjunto
/// chega: se os conjuntos das duas pontas se tocam, podem ser a mesma lingua e nao acusamos.
fn plausible_languages(text: &str) -> Vec<&'static str> {
    let ws = words(text);
    if ws.len() < MIN_WORDS_FOR_VERDICT {
        return Vec::new();
    }
    let scored: Vec<(usize, &'static str)> = STOPWORDS
        .iter()
        .map(|(lang, list)| {
            let hits = ws.iter().filter(|w| list.contains(&w.as_str())).count();
            (hits, *lang)
        })
        .collect();
    let best = scored.iter().map(|x| x.0).max().unwrap_or(0);
    if best < MIN_HITS {
        return Vec::new();
    }
    scored
        .into_iter()
        .filter(|(hits, _)| hits * 2 >= best)
        .map(|(_, lang)| lang)
        .collect()
}

/// `true` quando o output esta claramente noutra lingua que o input, ou seja, quando o modelo
/// traduziu apesar de o prompt o proibir. Ver a nota de desenho no topo desta seccao: em duvida
/// devolve `false`, porque recusar um refine bom custa mais do que deixar passar uma traducao.
pub fn language_flipped(input: &str, output: &str) -> bool {
    // Escrita diferente e prova por si so, e nao precisa de contagem de palavras nenhuma: um
    // texto em japones que volta em alfabeto latino foi traduzido.
    if let (Some(a), Some(b)) = (dominant_script(input), dominant_script(output)) {
        if a != b {
            return true;
        }
    }
    let a = plausible_languages(input);
    let b = plausible_languages(output);
    // Acusa so quando as duas pontas tem veredicto E nao ha nenhuma lingua que sirva as duas.
    !a.is_empty() && !b.is_empty() && !a.iter().any(|l| b.contains(l))
}

#[cfg(test)]
mod tests {
    use super::super::mask::{mask, scan_spans};
    use super::*;

    #[test]
    fn empty_and_whitespace_are_effectively_empty() {
        assert!(is_effectively_empty(""));
        assert!(is_effectively_empty("   \n\t "));
        assert!(!is_effectively_empty("x"));
    }

    #[test]
    fn preservation_passes_when_all_tokens_present() {
        let input = "run https://e.com/a and ```code```";
        let (_, table) = mask(input, &scan_spans(input));
        let out: String = table.tokens().collect::<Vec<_>>().join(" kept ");
        assert!(check_preservation(&table, &out));
    }

    #[test]
    fn preservation_fails_when_a_token_is_dropped() {
        let input = "run https://e.com/a now";
        let (_, table) = mask(input, &scan_spans(input));
        assert!(!check_preservation(&table, "run the command now"));
    }

    // --- guarda de lingua ---------------------------------------------------------------
    //
    // Metade destes testes existe para provar que a guarda NAO dispara. E o lado que importa:
    // um falso positivo recusa um refine bom e faz a app parecer partida, enquanto uma traducao
    // que escape o utilizador ve e desfaz.

    const PT: &str = "preciso de um resumo claro do que esta a acontecer com o sistema de \
        pagamentos, porque nao consigo perceber onde e que os pedidos estao a falhar e o que \
        e que devia mudar para isso deixar de acontecer todos os dias";
    const EN: &str = "i need a clear summary of what is going on with the payments system, \
        because i cannot work out where the requests are failing and what should change so \
        that this stops happening every single day of the week";

    #[test]
    fn flags_the_regression_that_started_this_a_portuguese_input_answered_in_english() {
        // Regressao real: um CLAUDE.md em portugues com "responder na lingua do utilizador"
        // fazia o refine TRADUZIR um input ingles. O prompt ja o proibia; isto e que o impede.
        assert!(language_flipped(PT, EN));
        assert!(language_flipped(EN, PT));
    }

    #[test]
    fn does_not_flag_a_normal_refine_in_the_same_language() {
        // O caso esmagadoramente comum: o mesmo texto, melhor escrito.
        let refined_pt = "Preciso de um resumo claro do que se passa no sistema de pagamentos: \
            nao consigo perceber onde e que os pedidos falham, nem o que deveria mudar para \
            que isso nao volte a acontecer todos os dias.";
        assert!(!language_flipped(PT, refined_pt));
        let refined_en = "I need a clear summary of what is happening with the payments \
            system. I cannot tell where the requests are failing, or what should change so \
            that this stops happening every day.";
        assert!(!language_flipped(EN, refined_en));
    }

    #[test]
    fn stays_quiet_on_short_text_where_there_is_no_evidence() {
        // Cinco palavras nao chegam para dizer em que lingua esta o texto. Em duvida, passa.
        assert!(!language_flipped("faz isto melhor", "make this better"));
        assert!(!language_flipped("ola", "hello"));
    }

    #[test]
    fn stays_quiet_between_languages_it_cannot_separate() {
        // Portugues e espanhol partilham demasiada palavra funcional. Em vez de escolher a
        // sorte, empata, e um empate deixa passar. Preferimos isso a recusar um refine bom.
        let es = "necesito un resumen claro de lo que esta pasando con el sistema de pagos, \
            porque no consigo entender donde estan fallando las peticiones y que deberia \
            cambiar para que esto deje de pasar todos los dias";
        assert!(!language_flipped(PT, es));
    }

    #[test]
    fn a_different_script_is_proof_on_its_own() {
        // Sem contar palavra nenhuma: um texto em japones que volta em alfabeto latino foi
        // traduzido, e nao ha outra leitura possivel.
        let ja =
            "決済システムの状況を明確にまとめてください。どこで失敗しているのかが分かりません。";
        assert!(language_flipped(
            ja,
            "please summarise the payment system status clearly"
        ));
        assert!(!language_flipped(ja, ja));
    }

    #[test]
    fn a_language_flip_degrades_instead_of_pasting_the_translation() {
        // Fim a fim pelo motor: o utilizador fica com o texto dele, nao com uma traducao.
        use super::super::{postprocess, precondition, DegradeReason, EngineResult};
        use crate::model::RefineMode;
        let prepared = precondition(PT, RefineMode::Adaptive);
        assert_eq!(
            postprocess(EN, &prepared),
            EngineResult::Degrade(DegradeReason::LanguageFlipped)
        );
    }

    #[test]
    fn preservation_trivially_passes_with_no_spans() {
        let table = SpanTable::default();
        assert!(check_preservation(&table, "any prose is fine"));
    }
}
