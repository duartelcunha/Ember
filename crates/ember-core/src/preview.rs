//! Stable comparison pages preserve every grapheme and bound visible lines.
use unicode_segmentation::UnicodeSegmentation;

pub fn pages(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut page = String::new();
    let mut columns = 0;
    let mut lines = 1;
    for grapheme in text.graphemes(true) {
        let newline = grapheme.contains('\n') || grapheme == "\r";
        let width = if grapheme == "\t" {
            4
        } else if grapheme.is_ascii() {
            1
        } else {
            2
        };
        if !newline && columns + width > 32 {
            lines += 1;
            columns = 0;
        }
        if lines > 8 {
            result.push(std::mem::take(&mut page));
            lines = 1;
            columns = 0;
        }
        page.push_str(grapheme);
        if newline {
            lines += 1;
            columns = 0;
        } else {
            columns += width;
        }
    }
    if !page.is_empty() || result.is_empty() {
        result.push(page);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_does_not_lose_bytes_or_split_joined_emoji() {
        let text = format!(
            "{}{}{}",
            "a".repeat(255),
            "👩‍💻".repeat(50),
            "\r\n".repeat(40)
        );
        let chunks = pages(&text);
        assert_eq!(chunks.concat(), text);
        assert!(chunks
            .iter()
            .all(|s| !s.starts_with('\u{200d}') && !s.ends_with('\u{200d}')));
    }
    #[test]
    fn newline_heavy_pages_remain_readable() {
        let chunks = pages(&"\n".repeat(400));
        assert_eq!(chunks.len(), 50);
        assert!(chunks.iter().all(|s| s.matches('\n').count() <= 8));
    }
    #[test]
    fn empty_comparison_still_has_one_page() {
        assert_eq!(pages(""), vec![""]);
    }
}
