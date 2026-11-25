//! Vietnamese street house numbering domain rules (odd/even sides + alley prefixes).

use ovg_core::AppError;
use serde::{Deserialize, Serialize};

use crate::prefix::canonical_prefix;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HouseSide {
    LeftOdd,
    RightEven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberingRule {
    pub street_name: String,
    pub alley_prefix: Option<String>,
    pub side: HouseSide,
    pub start_number: u32,
    pub step: u32,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberingProposal {
    pub sequence_index: usize,
    pub house_number: String,
    pub full_address: String,
    pub side: HouseSide,
}

/// Generate deterministic Vietnamese street/alley house numbers according to parity rules.
pub fn generate_street_numbers(rule: &NumberingRule) -> Result<Vec<NumberingProposal>, AppError> {
    let street = rule.street_name.trim();
    if street.is_empty() {
        return Err(AppError::validation(
            "INVALID_STREET_NAME",
            "street_name must not be empty",
        ));
    }
    if rule.start_number == 0 || rule.step == 0 || rule.count == 0 || rule.count > 10_000 {
        return Err(AppError::validation(
            "INVALID_NUMBERING_RULE",
            "start_number >= 1, step >= 1, and 1 <= count <= 10000 are required",
        ));
    }
    match rule.side {
        HouseSide::LeftOdd if rule.start_number % 2 == 0 => {
            return Err(AppError::validation(
                "PARITY_MISMATCH",
                "left_odd side requires an odd start_number",
            ));
        }
        HouseSide::RightEven if rule.start_number % 2 == 1 => {
            return Err(AppError::validation(
                "PARITY_MISMATCH",
                "right_even side requires an even start_number",
            ));
        }
        _ => {}
    }

    let prefix = match &rule.alley_prefix {
        Some(raw) if !raw.trim().is_empty() => Some(canonical_prefix(raw)?),
        _ => None,
    };

    let mut out = Vec::with_capacity(rule.count);
    for idx in 0..rule.count {
        let num = rule.start_number + (idx as u32) * rule.step;
        let house_number = match &prefix {
            Some(p) => format!("{p}/{num}"),
            None => num.to_string(),
        };
        let full_address = format!("{house_number} {street}");
        out.push(NumberingProposal {
            sequence_index: idx,
            house_number,
            full_address,
            side: rule.side,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_odd_and_even_alley_sequences() {
        let proposals = generate_street_numbers(&NumberingRule {
            street_name: "Đường Trần Hưng Đạo".to_string(),
            alley_prefix: Some("12/3a".to_string()),
            side: HouseSide::LeftOdd,
            start_number: 1,
            step: 2,
            count: 3,
        })
        .unwrap();

        assert_eq!(proposals.len(), 3);
        assert_eq!(proposals[0].house_number, "12/3A/1");
        assert_eq!(proposals[1].house_number, "12/3A/3");
        assert_eq!(proposals[2].full_address, "12/3A/5 Đường Trần Hưng Đạo");
    }

    #[test]
    fn rejects_wrong_side_parity() {
        let err = generate_street_numbers(&NumberingRule {
            street_name: "Phố Huế".to_string(),
            alley_prefix: None,
            side: HouseSide::RightEven,
            start_number: 1,
            step: 2,
            count: 2,
        })
        .unwrap_err();
        assert_eq!(err.code(), "PARITY_MISMATCH");
    }
}
