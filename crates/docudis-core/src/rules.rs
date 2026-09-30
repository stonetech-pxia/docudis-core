// Copyright 2026 the Docudis contributors.
//
// Licensed under Apache-2.0. Bundled rule data is derived from
// DocCloak.Core. See ../NOTICE and the source JSON files.

use crate::{validators, EntityType};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleCategory {
    Identity,
    Contact,
    Location,
    FinancialAccount,
    FinancialValue,
    Organization,
    BusinessRecord,
    MedicalRecord,
    LegalRecord,
    Digital,
    Credential,
    Temporal,
    GenericIdentifier,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleApplicability {
    Baseline,
    Specialized,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleProtectionLevel {
    Essential,
    Recommended,
    Optional,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleDefaultAction {
    Hide,
    DetectOnly,
    Contextual,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleProvenance {
    Doccloak,
    DoccloakModified,
    Docudis,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleStatus {
    Active,
    Experimental,
    Deprecated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleVertical {
    Healthcare,
    Legal,
    Finance,
    Employment,
    Insurance,
    Technology,
    Utilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulePackScope {
    #[serde(default)]
    pub jurisdictions: HashSet<String>,
    #[serde(default)]
    pub languages: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleClassification {
    pub category: RuleCategory,
    pub subtype: String,
    pub applicability: RuleApplicability,
    #[serde(default)]
    pub verticals: HashSet<RuleVertical>,
    pub protection_level: RuleProtectionLevel,
    pub default_action: RuleDefaultAction,
    pub provenance: RuleProvenance,
    pub status: RuleStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleSelection {
    pub jurisdictions: Option<HashSet<String>>,
    pub verticals: HashSet<RuleVertical>,
    pub categories: Option<HashSet<RuleCategory>>,
    pub enabled_rule_ids: HashSet<String>,
    pub disabled_rule_ids: HashSet<String>,
    pub include_all_specialized: bool,
}

impl RuleSelection {
    pub fn includes(&self, id: &str, classification: &RuleClassification) -> bool {
        if self.disabled_rule_ids.contains(id) {
            return false;
        }
        if self.enabled_rule_ids.contains(id) {
            return true;
        }
        if self
            .categories
            .as_ref()
            .is_some_and(|values| !values.contains(&classification.category))
        {
            return false;
        }
        classification.applicability == RuleApplicability::Baseline
            || self.include_all_specialized
            || classification
                .verticals
                .iter()
                .any(|v| self.verticals.contains(v))
    }
}

#[derive(Debug)]
pub struct RegexRule {
    pub id: String,
    pub region: String,
    pub entity_type: EntityType,
    pub pattern_source: String,
    pub flags: String,
    pub confidence: f64,
    pub description: String,
    pub scope: RulePackScope,
    pub classification: RuleClassification,
    pub examples: Vec<String>,
    pub validator_name: Option<String>,
    pub(crate) pattern: Regex,
    pub(crate) leading_guards: Vec<LookBehindGuard>,
}

#[derive(Debug)]
pub(crate) struct LookBehindGuard {
    pub positive: bool,
    pub pattern: Regex,
}

impl RegexRule {
    pub fn is_validated(&self) -> bool {
        self.validator_name.is_some()
    }
    pub fn validate(&self, value: &str) -> bool {
        self.validator_name
            .as_deref()
            .and_then(validators::by_name)
            .is_none_or(|v| v(value))
    }
}

#[derive(Debug)]
pub enum RuleError {
    Json(serde_json::Error),
    UnsupportedSchema(u32),
    UnknownEntity { rule: String, name: String },
    UnknownValidator { rule: String, name: String },
    Regex { rule: String, message: String },
}
impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid rule pack JSON: {e}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported rule schema version {v}"),
            Self::UnknownEntity { rule, name } => {
                write!(f, "rule {rule}: unknown entity type {name}")
            }
            Self::UnknownValidator { rule, name } => {
                write!(f, "rule {rule}: unknown validator {name}")
            }
            Self::Regex { rule, message } => {
                write!(f, "rule {rule}: pattern does not compile: {message}")
            }
        }
    }
}
impl Error for RuleError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPack {
    schema_version: u32,
    region: String,
    scope: RulePackScope,
    rules: Vec<RawRule>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    id: String,
    entity_type: String,
    pattern: String,
    #[serde(default = "global_flag")]
    flags: String,
    confidence: f64,
    description: String,
    classification: RuleClassification,
    #[serde(default)]
    examples: Vec<String>,
    validate: Option<String>,
}
fn global_flag() -> String {
    "g".to_owned()
}

pub fn parse_rule_pack(source: &str) -> Result<Vec<RegexRule>, RuleError> {
    let pack: RawPack = serde_json::from_str(source).map_err(RuleError::Json)?;
    if pack.schema_version != 2 {
        return Err(RuleError::UnsupportedSchema(pack.schema_version));
    }
    let mut out = Vec::with_capacity(pack.rules.len());
    for raw in pack.rules {
        let entity_type =
            EntityType::from_name(&raw.entity_type).ok_or_else(|| RuleError::UnknownEntity {
                rule: raw.id.clone(),
                name: raw.entity_type.clone(),
            })?;
        if let Some(name) = raw.validate.as_deref() {
            if validators::by_name(name).is_none() {
                return Err(RuleError::UnknownValidator {
                    rule: raw.id.clone(),
                    name: name.to_owned(),
                });
            }
        }
        let mut modes = String::new();
        if raw.flags.contains('i') {
            modes.push('i')
        }
        if raw.flags.contains('m') {
            modes.push('m')
        }
        if raw.flags.contains('s') {
            modes.push('s')
        }
        let (candidate, raw_guards) = strip_leading_lookbehinds(&raw.pattern);
        let compiled = if modes.is_empty() {
            candidate.to_owned()
        } else {
            format!("(?{modes}){candidate}")
        };
        let pattern = Regex::new(&compiled).map_err(|e| RuleError::Regex {
            rule: raw.id.clone(),
            message: e.to_string(),
        })?;
        let leading_guards = raw_guards
            .into_iter()
            .map(|(positive, source)| {
                let guarded = if modes.is_empty() {
                    format!("(?:{source})$")
                } else {
                    format!("(?{modes})(?:{source})$")
                };
                Regex::new(&guarded)
                    .map(|pattern| LookBehindGuard { positive, pattern })
                    .map_err(|e| RuleError::Regex {
                        rule: raw.id.clone(),
                        message: format!("lookbehind guard: {e}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        out.push(RegexRule {
            id: raw.id,
            region: pack.region.clone(),
            entity_type,
            pattern_source: raw.pattern,
            flags: raw.flags,
            confidence: raw.confidence,
            description: raw.description,
            scope: pack.scope.clone(),
            classification: raw.classification,
            examples: raw.examples,
            validator_name: raw.validate,
            pattern,
            leading_guards,
        });
    }
    Ok(out)
}

/// Rust's backtracking engine requires fixed-width lookbehind. Rule packs use
/// several bounded variable-width label assertions. Turn only *leading*
/// lookbehinds into explicit prefix guards; the consuming candidate remains
/// unchanged, so offsets stay identical to Dart.
fn strip_leading_lookbehinds(mut source: &str) -> (&str, Vec<(bool, &str)>) {
    let mut guards = Vec::new();
    loop {
        let (positive, body_start) = if source.starts_with("(?<=") {
            (true, 4)
        } else if source.starts_with("(?<!") {
            (false, 4)
        } else {
            break;
        };
        let bytes = source.as_bytes();
        let mut depth = 1_usize;
        let mut escaped = false;
        let mut class = false;
        let mut end = None;
        for (index, byte) in bytes.iter().copied().enumerate().skip(body_start) {
            if escaped {
                escaped = false;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
            } else if byte == b'[' {
                class = true;
            } else if byte == b']' {
                class = false;
            } else if !class && byte == b'(' {
                depth += 1;
            } else if !class && byte == b')' {
                depth -= 1;
                if depth == 0 {
                    end = Some(index);
                    break;
                }
            }
        }
        let Some(end) = end else { break };
        guards.push((positive, &source[body_start..end]));
        source = &source[end + 1..];
    }
    (source, guards)
}

pub const BUNDLED_RULE_PACKS: &[(&str, &str)] = &[
    (
        "universal",
        include_str!("../../../data/rules/universal.json"),
    ),
    ("at", include_str!("../../../data/rules/at.json")),
    ("be", include_str!("../../../data/rules/be.json")),
    ("ch", include_str!("../../../data/rules/ch.json")),
    ("cn", include_str!("../../../data/rules/cn.json")),
    ("de", include_str!("../../../data/rules/de.json")),
    ("dk", include_str!("../../../data/rules/dk.json")),
    ("es", include_str!("../../../data/rules/es.json")),
    ("fi", include_str!("../../../data/rules/fi.json")),
    ("fr", include_str!("../../../data/rules/fr.json")),
    ("gb", include_str!("../../../data/rules/gb.json")),
    ("ie", include_str!("../../../data/rules/ie.json")),
    ("it", include_str!("../../../data/rules/it.json")),
    ("jp", include_str!("../../../data/rules/jp.json")),
    ("nl", include_str!("../../../data/rules/nl.json")),
    ("no", include_str!("../../../data/rules/no.json")),
    ("pl", include_str!("../../../data/rules/pl.json")),
    ("pt", include_str!("../../../data/rules/pt.json")),
    ("se", include_str!("../../../data/rules/se.json")),
    ("us", include_str!("../../../data/rules/us.json")),
];

pub fn bundled_rules(
    regions: Option<&HashSet<String>>,
    selection: Option<&RuleSelection>,
) -> Result<Vec<RegexRule>, RuleError> {
    let selected = selection.and_then(|s| s.jurisdictions.as_ref()).or(regions);
    let mut out = Vec::new();
    for (region, source) in BUNDLED_RULE_PACKS {
        if *region != "universal"
            && selected
                .is_some_and(|set| !set.iter().any(|value| value.eq_ignore_ascii_case(region)))
        {
            continue;
        }
        for rule in parse_rule_pack(source)? {
            if selection.is_none_or(|s| s.includes(&rule.id, &rule.classification)) {
                out.push(rule)
            }
        }
    }
    Ok(out)
}

pub fn regions_for_languages<'a>(
    tags: impl IntoIterator<Item = &'a str>,
    text: &str,
) -> HashSet<String> {
    let mut langs = HashSet::new();
    for tag in tags {
        langs.insert(
            tag.split(['-', '_'])
                .next()
                .unwrap_or(tag)
                .to_ascii_lowercase(),
        );
    }
    let mut out = HashSet::new();
    for lang in &langs {
        let values: &[&str] = match lang.as_str() {
            "zh" => &["cn"],
            "en" => &["us", "gb", "ie"],
            "fr" => &["fr", "be", "ch"],
            "es" => &["es"],
            "de" => &["de", "at", "ch"],
            "it" => &["it", "ch"],
            "pt" => &["pt"],
            "nl" => &["nl", "be"],
            "pl" => &["pl"],
            "sv" => &["se"],
            "no" | "nb" | "nn" => &["no"],
            "da" => &["dk"],
            "fi" => &["fi"],
            "ja" => &["jp"],
            "hi" => &[],
            _ => continue,
        };
        out.extend(values.iter().map(|s| (*s).to_owned()));
    }
    if !langs.iter().any(|l| {
        matches!(
            l.as_str(),
            "zh" | "en"
                | "fr"
                | "es"
                | "de"
                | "it"
                | "pt"
                | "nl"
                | "pl"
                | "sv"
                | "no"
                | "nb"
                | "nn"
                | "da"
                | "fi"
                | "ja"
                | "hi"
        )
    }) {
        out.extend(["us", "gb", "fr", "es"].map(str::to_owned));
    }
    if !langs.contains("ja") && text.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)) {
        out.insert("cn".to_owned());
    }
    out
}
