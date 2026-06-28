/// Normalise a language tag from a Calibre OPF `<dc:language>` element.
///
/// Calibre emits IETF BCP-47 (`ja`, `en-US`) but also ISO 639-2 (`jpn`, `eng`).
/// Returns the LingQ-facing 2-letter code; unknown inputs pass through the
/// first 2 chars lowercased.
pub fn normalise(raw: &str) -> &'static str {
    let lower = raw.trim().to_lowercase();
    let head = lower.split(['-', '_']).next().unwrap_or("");
    match head {
        "ja" | "jpn" | "jap" => "ja",
        "en" | "eng" => "en",
        "zh" | "chi" | "zho" => "zh",
        "fr" | "fra" | "fre" => "fr",
        "de" | "ger" | "deu" => "de",
        "es" | "spa" => "es",
        "it" | "ita" => "it",
        "pt" | "por" => "pt",
        "ru" | "rus" => "ru",
        "ko" | "kor" => "ko",
        _ if head.len() >= 2 => leak2(&head[..2]),
        _ => "",
    }
}

fn leak2(s: &str) -> &'static str {
    match s {
        "aa" => "aa",
        "ab" => "ab",
        "af" => "af",
        "ak" => "ak",
        "ar" => "ar",
        "as" => "as",
        "az" => "az",
        "be" => "be",
        "bg" => "bg",
        "bn" => "bn",
        "bo" => "bo",
        "bs" => "bs",
        "ca" => "ca",
        "cs" => "cs",
        "cy" => "cy",
        "da" => "da",
        "el" => "el",
        "et" => "et",
        "fa" => "fa",
        "fi" => "fi",
        "ga" => "ga",
        "gl" => "gl",
        "gu" => "gu",
        "he" => "he",
        "hi" => "hi",
        "hr" => "hr",
        "hu" => "hu",
        "hy" => "hy",
        "id" => "id",
        "is" => "is",
        "ka" => "ka",
        "kk" => "kk",
        "km" => "km",
        "kn" => "kn",
        "ky" => "ky",
        "la" => "la",
        "lo" => "lo",
        "lt" => "lt",
        "lv" => "lv",
        "mk" => "mk",
        "ml" => "ml",
        "mn" => "mn",
        "mr" => "mr",
        "ms" => "ms",
        "my" => "my",
        "ne" => "ne",
        "nl" => "nl",
        "no" => "no",
        "pa" => "pa",
        "pl" => "pl",
        "ro" => "ro",
        "si" => "si",
        "sk" => "sk",
        "sl" => "sl",
        "so" => "so",
        "sq" => "sq",
        "sr" => "sr",
        "sv" => "sv",
        "sw" => "sw",
        "ta" => "ta",
        "te" => "te",
        "th" => "th",
        "tl" => "tl",
        "tr" => "tr",
        "uk" => "uk",
        "ur" => "ur",
        "uz" => "uz",
        "vi" => "vi",
        "yi" => "yi",
        "zu" => "zu",
        _ => "xx",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpn_iso_639_2_maps_to_ja() {
        assert_eq!(normalise("jpn"), "ja");
        assert_eq!(normalise("JA"), "ja");
        assert_eq!(normalise("ja-JP"), "ja");
    }

    #[test]
    fn english_variants_fold_to_en() {
        assert_eq!(normalise("en"), "en");
        assert_eq!(normalise("en-US"), "en");
        assert_eq!(normalise("eng"), "en");
    }

    #[test]
    fn chinese_fold_to_zh() {
        assert_eq!(normalise("zh-Hans"), "zh");
        assert_eq!(normalise("chi"), "zh");
    }
}
