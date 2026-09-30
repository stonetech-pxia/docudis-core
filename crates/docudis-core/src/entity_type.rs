// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// A stable kind of sensitive information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntityType {
    Person,
    Email,
    Phone,
    #[serde(alias = "SSN")]
    Id,
    Number,
    #[serde(alias = "CREDIT_CARD")]
    Card,
    Iban,
    Date,
    BirthDate,
    #[serde(alias = "CURRENCY")]
    Amount,
    #[serde(alias = "IP_ADDRESS")]
    Ip,
    Url,
    Address,
    Company,
    Secret,
    ApiKey,
    Custom,
    Other,
}

impl EntityType {
    pub const ALL: [Self; 18] = [
        Self::Person,
        Self::Email,
        Self::Phone,
        Self::Id,
        Self::Number,
        Self::Card,
        Self::Iban,
        Self::Date,
        Self::BirthDate,
        Self::Amount,
        Self::Ip,
        Self::Url,
        Self::Address,
        Self::Company,
        Self::Secret,
        Self::ApiKey,
        Self::Custom,
        Self::Other,
    ];

    pub const fn placeholder_name(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::Id => "ID",
            Self::Number => "NUMBER",
            Self::Card => "CARD",
            Self::Iban => "IBAN",
            Self::Date => "DATE",
            Self::BirthDate => "BIRTH_DATE",
            Self::Amount => "AMOUNT",
            Self::Ip => "IP",
            Self::Url => "URL",
            Self::Address => "ADDRESS",
            Self::Company => "COMPANY",
            Self::Secret => "SECRET",
            Self::ApiKey => "API_KEY",
            Self::Custom => "CUSTOM",
            Self::Other => "OTHER",
        }
    }

    /// Resolves stable placeholder names and DocCloak rule-pack aliases.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "SSN" => Some(Self::Id),
            "CREDIT_CARD" => Some(Self::Card),
            "CURRENCY" => Some(Self::Amount),
            "IP_ADDRESS" => Some(Self::Ip),
            _ => Self::ALL
                .into_iter()
                .find(|entity_type| entity_type.placeholder_name() == name),
        }
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.placeholder_name())
    }
}

impl FromStr for EntityType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_name(value).ok_or_else(|| format!("unknown entity type: {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::EntityType;

    #[test]
    fn resolves_stable_names_and_legacy_aliases() {
        assert_eq!(EntityType::from_name("PERSON"), Some(EntityType::Person));
        assert_eq!(EntityType::from_name("SSN"), Some(EntityType::Id));
        assert_eq!(EntityType::from_name("CREDIT_CARD"), Some(EntityType::Card));
        assert_eq!(EntityType::from_name("not-a-type"), None);
    }
}
