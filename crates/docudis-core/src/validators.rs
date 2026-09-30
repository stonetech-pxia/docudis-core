// Copyright 2026 the Docudis contributors.
//
// Licensed under Apache-2.0. Validator behaviour is ported from
// DocCloak.Core and the Dart reference. See ../NOTICE.

use std::collections::HashMap;

pub type RuleValidator = fn(&str) -> bool;

fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

pub fn luhn(value: &str) -> bool {
    let clean: String = value.chars().filter(|c| *c != ' ' && *c != '-').collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0;
    let mut alternate = false;
    for mut n in clean.bytes().rev().map(|c| (c - b'0') as u32) {
        if alternate {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        sum += n;
        alternate = !alternate;
    }
    sum % 10 == 0
}

pub fn iban_mod97(value: &str) -> bool {
    let clean: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if !(5..=34).contains(&clean.len()) || !clean.is_ascii() {
        return false;
    }
    let rearranged = format!("{}{}", &clean[4..], &clean[..4]);
    let mut remainder = 0_u32;
    for ch in rearranged.bytes() {
        if ch.is_ascii_digit() {
            remainder = (remainder * 10 + u32::from(ch - b'0')) % 97;
        } else if ch.is_ascii_uppercase() {
            let value = u32::from(ch - b'A') + 10;
            remainder = (remainder * 100 + value) % 97;
        } else {
            return false;
        }
    }
    remainder == 1
}

fn ip_octets(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 4 && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

fn nino(value: &str) -> bool {
    let clean: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    let bytes = clean.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    !b"DFIQUV".contains(&bytes[0])
        && !b"DFIOQUV".contains(&bytes[1])
        && !matches!(&clean[..2], "BG" | "GB" | "NK" | "KN" | "TN" | "NT" | "ZZ")
}

fn weighted_check(value: &str, weights: &[i32], check_index: usize, modulus: i32) -> bool {
    let d = digits(value);
    if d.len() <= check_index || weights.len() > d.len() {
        return false;
    }
    let sum: i32 = weights
        .iter()
        .enumerate()
        .map(|(i, weight)| i32::from(d.as_bytes()[i] - b'0') * weight)
        .sum();
    sum.rem_euclid(modulus) == i32::from(d.as_bytes()[check_index] - b'0')
}

fn nhs(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 10 {
        return false;
    }
    let sum: u32 = [10, 9, 8, 7, 6, 5, 4, 3, 2]
        .iter()
        .enumerate()
        .map(|(i, w)| u32::from(d.as_bytes()[i] - b'0') * w)
        .sum();
    let check = 11 - sum % 11;
    (check == 11 && d.as_bytes()[9] == b'0')
        || (check < 10 && check == u32::from(d.as_bytes()[9] - b'0'))
}

fn uk_driving_licence(value: &str) -> bool {
    let clean: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() != 16 || !clean.is_ascii() {
        return false;
    }
    let dob = &clean[5..11];
    if !dob.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let raw_month: u8 = dob[1..3].parse().unwrap_or(0);
    let month = if raw_month > 50 {
        raw_month - 50
    } else {
        raw_month
    };
    let day: u8 = dob[3..5].parse().unwrap_or(0);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

fn pesel(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 11 {
        return false;
    }
    let sum: u32 = [1, 3, 7, 9, 1, 3, 7, 9, 1, 3]
        .iter()
        .enumerate()
        .map(|(i, w)| u32::from(d.as_bytes()[i] - b'0') * w)
        .sum();
    (10 - sum % 10) % 10 == u32::from(d.as_bytes()[10] - b'0')
}

fn pl_id_card(value: &str) -> bool {
    let clean: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if clean.len() != 9 || !clean.is_ascii() {
        return false;
    }
    let weights = [7, 3, 1, 0, 7, 3, 1, 7, 3];
    let mut sum = 0_u32;
    for (i, b) in clean.bytes().enumerate() {
        if i == 3 {
            continue;
        }
        let n = if b.is_ascii_uppercase() {
            u32::from(b - b'A') + 10
        } else if b.is_ascii_digit() {
            u32::from(b - b'0')
        } else {
            return false;
        };
        sum += n * weights[i];
    }
    clean.as_bytes()[3].is_ascii_digit() && sum % 10 == u32::from(clean.as_bytes()[3] - b'0')
}

fn nip(value: &str) -> bool {
    weighted_check(value, &[6, 5, 7, 2, 3, 4, 5, 6, 7], 9, 11)
}

fn regon(value: &str) -> bool {
    let d = digits(value);
    let weights: &[u32] = match d.len() {
        9 => &[8, 9, 2, 3, 4, 5, 6, 7],
        14 => &[2, 4, 8, 5, 0, 9, 7, 3, 6, 1, 2, 4, 8],
        _ => return false,
    };
    let sum: u32 = weights
        .iter()
        .enumerate()
        .map(|(i, w)| u32::from(d.as_bytes()[i] - b'0') * w)
        .sum();
    let rem = sum % 11;
    let check = if rem == 10 { 0 } else { rem };
    check == u32::from(*d.as_bytes().last().unwrap() - b'0')
}

fn personnummer(value: &str) -> bool {
    let clean: String = value.chars().filter(|c| *c != ' ' && *c != '-').collect();
    let day_clean: String = value.chars().filter(|c| *c != '-' && *c != '+').collect();
    if clean.len() < 10 || day_clean.len() < 6 || day_clean[4..6].parse::<u8>().unwrap_or(99) >= 61
    {
        return false;
    }
    luhn(&clean[clean.len() - 10..])
}

fn samordningsnummer(value: &str) -> bool {
    let clean: String = value.chars().filter(|c| *c != '-' && *c != '+').collect();
    clean.len() >= 10 && clean[4..6].parse::<u8>().is_ok_and(|day| day >= 61)
}

fn dea(value: &str) -> bool {
    let clean = value.to_ascii_uppercase();
    let b = clean.as_bytes();
    if b.len() != 9
        || !b"ABCDEFGHJKLMPRSTUX".contains(&b[0])
        || !b[1].is_ascii_uppercase()
        || !b[2..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let d: Vec<u32> = b[2..].iter().map(|v| u32::from(v - b'0')).collect();
    (d[0] + d[2] + d[4] + 2 * (d[1] + d[3] + d[5])) % 10 == d[6]
}

fn npi(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 10 {
        return false;
    }
    let payload = format!("80840{}", &d[..9]);
    let mut sum = 0;
    let mut alt = true;
    for b in payload.bytes().rev() {
        let mut n = u32::from(b - b'0');
        if alt {
            n *= 2;
            if n > 9 {
                n -= 9
            }
        }
        sum += n;
        alt = !alt
    }
    (10 - sum % 10) % 10 == u32::from(d.as_bytes()[9] - b'0')
}

fn aba(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 9 {
        return false;
    };
    let n: Vec<u32> = d.bytes().map(|b| u32::from(b - b'0')).collect();
    (3 * (n[0] + n[3] + n[6]) + 7 * (n[1] + n[4] + n[7]) + n[2] + n[5] + n[8]).is_multiple_of(10)
}

fn nir(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 15 || !matches!(d.as_bytes()[0], b'1' | b'2') {
        return false;
    }
    let a = d[..13].parse::<u64>().ok();
    let k = d[13..].parse::<u64>().ok();
    matches!((a,k),(Some(a),Some(k)) if 97-a%97==k)
}
fn dni(value: &str) -> bool {
    let c: String = value
        .chars()
        .filter(|x| *x != ' ' && *x != '-')
        .flat_map(char::to_uppercase)
        .collect();
    if c.len() < 2 || !c.is_ascii() {
        return false;
    }
    let Some(n) = c[..c.len() - 1].parse::<usize>().ok() else {
        return false;
    };
    b"TRWAGMYFPDXBNJZSQVHLCKE"[n % 23] == c.as_bytes()[c.len() - 1]
}
fn cups(value: &str) -> bool {
    let c: String = value
        .chars()
        .filter(|x| !x.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();
    if c.len() < 20 || !c.is_ascii() {
        return false;
    }
    let Some(n) = c[2..18].parse::<usize>().ok() else {
        return false;
    };
    let r = n % 529;
    let l = b"TRWAGMYFPDXBNJZSQVHLCKE";
    c.as_bytes()[18] == l[r / 23] && c.as_bytes()[19] == l[r % 23]
}

fn codice_fiscale(value: &str) -> bool {
    const O: [i32; 36] = [
        1, 0, 5, 7, 9, 13, 15, 17, 19, 21, 1, 0, 5, 7, 9, 13, 15, 17, 19, 21, 2, 4, 18, 20, 11, 3,
        6, 8, 12, 14, 16, 10, 22, 25, 24, 23,
    ];
    let c = value.to_ascii_uppercase();
    if c.len() != 16 || !c.is_ascii() {
        return false;
    }
    let mut sum = 0;
    for (i, b) in c.bytes().take(15).enumerate() {
        let idx = if b.is_ascii_digit() {
            usize::from(b - b'0')
        } else if b.is_ascii_uppercase() {
            usize::from(b - b'A') + 10
        } else {
            return false;
        };
        sum += if i % 2 == 0 {
            O[idx]
        } else if idx < 10 {
            idx as i32
        } else {
            idx as i32 - 10
        }
    }
    c.as_bytes()[15] == b'A' + (sum % 26) as u8
}

fn partita_iva(value: &str) -> bool {
    let mut c = value.to_ascii_uppercase();
    if let Some(v) = c.strip_prefix("IT") {
        c = v.to_owned()
    }
    if c.len() != 11 || !c.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0;
    for (i, b) in c.bytes().enumerate() {
        let mut n = u32::from(b - b'0');
        if i % 2 == 1 {
            n *= 2;
            if n > 9 {
                n -= 9
            }
        }
        sum += n
    }
    sum % 10 == 0
}
fn bsn(value: &str) -> bool {
    let d = digits(value);
    d.len() == 9 && {
        let w = [9, 8, 7, 6, 5, 4, 3, 2, -1];
        let s: i32 = d
            .bytes()
            .enumerate()
            .map(|(i, b)| i32::from(b - b'0') * w[i])
            .sum();
        s > 0 && s % 11 == 0
    }
}
fn nif(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 9 {
        return false;
    }
    let first = d.as_bytes()[0] - b'0';
    if matches!(first, 0 | 3 | 4 | 7) {
        return false;
    }
    let w = [9, 8, 7, 6, 5, 4, 3, 2];
    let sum: u32 = w
        .iter()
        .enumerate()
        .map(|(i, x)| u32::from(d.as_bytes()[i] - b'0') * x)
        .sum();
    let r = sum % 11;
    let c = if r < 2 { 0 } else { 11 - r };
    c == u32::from(d.as_bytes()[8] - b'0')
}
fn fodselsnummer(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 11 || d[..2].parse::<u8>().unwrap_or(99) >= 41 {
        return false;
    }
    let n: Vec<i32> = d.bytes().map(|b| i32::from(b - b'0')).collect();
    let k1 = 11
        - (3 * n[0]
            + 7 * n[1]
            + 6 * n[2]
            + n[3]
            + 8 * n[4]
            + 9 * n[5]
            + 4 * n[6]
            + 5 * n[7]
            + 2 * n[8])
            % 11;
    let c1 = if k1 == 11 { 0 } else { k1 };
    if c1 == 10 || c1 != n[9] {
        return false;
    }
    let k2 = 11
        - (5 * n[0]
            + 4 * n[1]
            + 3 * n[2]
            + 2 * n[3]
            + 7 * n[4]
            + 6 * n[5]
            + 5 * n[6]
            + 4 * n[7]
            + 3 * n[8]
            + 2 * n[9])
            % 11;
    let c2 = if k2 == 11 { 0 } else { k2 };
    c2 != 10 && c2 == n[10]
}
fn d_nummer(value: &str) -> bool {
    let d = digits(value);
    d.len() == 11 && d[..2].parse::<u8>().is_ok_and(|x| (41..=71).contains(&x))
}
fn belgian_nrn(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 11 {
        return false;
    }
    let a = d[..9].parse::<u64>().unwrap();
    let k = d[9..].parse::<u64>().unwrap();
    97 - a % 97 == k || 97 - format!("2{}", &d[..9]).parse::<u64>().unwrap() % 97 == k
}
fn svnr(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 10 || d[..3].parse::<u16>().unwrap_or(0) < 100 {
        return false;
    }
    let w = [3, 7, 9, 0, 5, 8, 4, 2, 1, 6];
    let s: u32 = d
        .bytes()
        .enumerate()
        .filter(|(i, _)| *i != 3)
        .map(|(i, b)| u32::from(b - b'0') * w[i])
        .sum();
    s % 11 != 10 && s % 11 == u32::from(d.as_bytes()[3] - b'0')
}
fn ahv(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 13 || !d.starts_with("756") {
        return false;
    }
    let s: u32 = d
        .bytes()
        .take(12)
        .enumerate()
        .map(|(i, b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    (10 - s % 10) % 10 == u32::from(d.as_bytes()[12] - b'0')
}
fn steuer_id(value: &str) -> bool {
    let d = digits(value);
    if d.len() != 11 || d.starts_with('0') {
        return false;
    }
    let mut p = 10;
    for b in d.bytes().take(10) {
        let mut s = (u32::from(b - b'0') + p) % 10;
        if s == 0 {
            s = 10
        }
        p = s * 2 % 11
    }
    (11 - p) % 10 == u32::from(d.as_bytes()[10] - b'0')
}
fn hetu(value: &str) -> bool {
    let c = value.to_ascii_uppercase();
    if c.len() < 11 || !c.is_ascii() {
        return false;
    }
    let Ok(n) = format!("{}{}", &c[..6], &c[7..10]).parse::<usize>() else {
        return false;
    };
    b"0123456789ABCDEFHJKLMNPRSTUVWXY"[n % 31] == c.as_bytes()[10]
}
fn pps(value: &str) -> bool {
    let c = value.to_ascii_uppercase();
    if c.len() < 8 || !c.is_ascii() {
        return false;
    }
    let w = [8, 7, 6, 5, 4, 3, 2];
    let mut sum = 0_u32;
    for (i, b) in c.bytes().take(7).enumerate() {
        if !b.is_ascii_digit() {
            return false;
        }
        sum += u32::from(b - b'0') * w[i]
    }
    if c.len() > 8 && c.as_bytes()[8] != b'W' {
        sum += u32::from(c.as_bytes()[8] - b'A' + 1) * 9
    }
    let r = sum % 23;
    let expected = if r == 0 { b'W' } else { b'A' + r as u8 - 1 };
    c.as_bytes()[7] == expected
}

fn entropy(value: &str) -> f64 {
    if value.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<char, f64> = HashMap::new();
    let mut n = 0.0_f64;
    for c in value.chars() {
        *counts.entry(c).or_insert(0.0) += 1.0;
        n += 1.0
    }
    counts
        .values()
        .map(|count| {
            let p = count / n;
            -p * p.log2()
        })
        .sum()
}
fn secret_assignment(value: &str) -> bool {
    let Some(i) = value.find([':', '=']) else {
        return false;
    };
    let v = value[i + 1..]
        .trim_start_matches(['>', ' ', '\t', '\n', '\r', '"', '\''])
        .trim_end_matches('=');
    v.len() >= 16
        && !v.bytes().all(|b| b.is_ascii_digit())
        && if v.bytes().all(|b| b.is_ascii_hexdigit()) {
            entropy(v) >= 3.0
        } else {
            entropy(v) >= 4.0
        }
}
fn high_entropy_token(value: &str) -> bool {
    let v = value.trim_end_matches('=');
    (40..=256).contains(&v.len())
        && v.chars().any(|c| c.is_ascii_lowercase())
        && v.chars().any(|c| c.is_ascii_uppercase())
        && v.chars().any(|c| c.is_ascii_digit())
        && entropy(v) >= 4.5
}

pub fn by_name(name: &str) -> Option<RuleValidator> {
    Some(match name {
        "aba" => aba,
        "ahv" => ahv,
        "belgianNrn" => belgian_nrn,
        "bsn" => bsn,
        "codiceFiscale" => codice_fiscale,
        "cups" => cups,
        "dNummer" => d_nummer,
        "dea" => dea,
        "dni" => dni,
        "fodselsnummer" => fodselsnummer,
        "hetu" => hetu,
        "highEntropyToken" => high_entropy_token,
        "ibanMod97" => iban_mod97,
        "ipOctets" => ip_octets,
        "luhn" => luhn,
        "nhs" => nhs,
        "nif" => nif,
        "nino" => nino,
        "nip" => nip,
        "nir" => nir,
        "npi" => npi,
        "partitaIva" => partita_iva,
        "personnummer" => personnummer,
        "pesel" => pesel,
        "plIdCard" => pl_id_card,
        "pps" => pps,
        "regon" => regon,
        "samordningsnummer" => samordningsnummer,
        "secretAssignment" => secret_assignment,
        "steuerId" => steuer_id,
        "svnr" => svnr,
        "ukDrivingLicence" => uk_driving_licence,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_checksums() {
        assert!(luhn("4111 1111 1111 1111"));
        assert!(!luhn("4111 1111 1111 1112"));
        assert!(iban_mod97("GB29 NWBK 6016 1331 9268 19"));
        assert!(!iban_mod97("GB29 NWBK 6016 1331 9268 18"));
        assert!(ip_octets("192.168.1.1"));
        assert!(!ip_octets("999.1.1.1"));
    }
}
