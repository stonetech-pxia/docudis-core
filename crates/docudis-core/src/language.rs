// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

//! Language identification for hosts that have none of their own (Windows;
//! Android uses ML Kit). Only built with the `language-id` feature.

use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use std::sync::LazyLock;

/// The languages Docudis has rule packs for, plus common languages that
/// would otherwise be taken for one of them (Turkish for German, Catalan for
/// Spanish) and so switch on the wrong packs. Must match the `lingua`
/// features in the workspace manifest.
const LANGUAGES: [Language; 30] = [
    Language::Chinese,
    Language::English,
    Language::French,
    Language::Spanish,
    Language::German,
    Language::Italian,
    Language::Portuguese,
    Language::Dutch,
    Language::Polish,
    Language::Swedish,
    Language::Bokmal,
    Language::Nynorsk,
    Language::Danish,
    Language::Finnish,
    Language::Japanese,
    Language::Hindi,
    Language::Russian,
    Language::Ukrainian,
    Language::Arabic,
    Language::Korean,
    Language::Turkish,
    Language::Vietnamese,
    Language::Indonesian,
    Language::Czech,
    Language::Greek,
    Language::Romanian,
    Language::Hungarian,
    Language::Hebrew,
    Language::Thai,
    Language::Catalan,
];

/// Same cut-off as the Android app's ML Kit `identifyPossibleLanguages`.
const MIN_CONFIDENCE: f64 = 0.3;

/// Models load on first use and stay for the life of the process.
static DETECTOR: LazyLock<LanguageDetector> =
    LazyLock::new(|| LanguageDetectorBuilder::from_languages(&LANGUAGES).build());

/// ISO 639-1 tags of the languages plausibly present in `text`, most likely
/// first; empty when none is likely enough. Feed them to
/// [`crate::regions_for_languages`].
pub fn detect_languages(text: &str) -> Vec<String> {
    DETECTOR
        .compute_language_confidence_values(text)
        .into_iter()
        .filter(|(_, confidence)| *confidence >= MIN_CONFIDENCE)
        .map(|(language, _)| language.iso_code_639_1().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_the_main_language() {
        assert_eq!(
            detect_languages("Le locataire paie le loyer chaque mois par virement."),
            ["fr"]
        );
        assert_eq!(detect_languages("租客每月通过银行转账支付房租。"), ["zh"]);
        assert_eq!(
            detect_languages("Jeg bor i Oslo og jobber som ingeniør i et lite firma."),
            ["nb"]
        );
    }

    #[test]
    fn other_languages_are_not_taken_for_one_with_rules() {
        let tags = detect_languages(
            "Merhaba, benim adım Mehmet. İstanbul'da yaşıyorum ve mühendis olarak çalışıyorum.",
        );
        assert_eq!(tags, ["tr"]);
    }

    #[test]
    fn nothing_to_go_by_gives_no_tag() {
        assert!(detect_languages("").is_empty());
        assert!(detect_languages("12345 !!!").is_empty());
    }
}
