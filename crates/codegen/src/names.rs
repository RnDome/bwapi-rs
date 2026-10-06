//! Mechanical name conversions. Everything else is written explicitly in bwapi.api.

/// `Terran_Marine` → `TERRAN_MARINE`, `AttackMove` → `ATTACK_MOVE`, `Terran_SCV` → `TERRAN_SCV`.
pub fn screaming_snake(s: &str) -> String {
    snake(s).to_uppercase()
}

/// `UnitType` → `unit_type`, `AIModule` → `ai_module`.
pub fn snake(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev.is_ascii_lowercase() || (prev.is_ascii_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// `shift_queue_command` → `shiftQueueCommand`: a parameter name as BWAPI writes it.
pub fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in s.chars() {
        if c == '_' {
            upper = !out.is_empty();
        } else if upper {
            out.push(c.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Prefix of C names in a C++ scope: `BWAPI::UnitTypes::Enum` → `UnitTypes`, `BWAPI` → `BWAPI`.
pub fn c_scope(cpp: &str) -> &str {
    let scope = cpp.strip_suffix("::Enum").unwrap_or(cpp);
    scope.rsplit("::").next().unwrap_or(scope)
}

/// `LButton` yes; `L_BUTTON`, `lButton` no.
pub fn is_upper_camel(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(screaming_snake("Terran_Marine"), "TERRAN_MARINE");
        assert_eq!(screaming_snake("Terran_SCV"), "TERRAN_SCV");
        assert_eq!(screaming_snake("AttackMove"), "ATTACK_MOVE");
        assert_eq!(screaming_snake("Unknown_0x0E"), "UNKNOWN_0X0E");
        assert_eq!(screaming_snake("CTF_COP"), "CTF_COP");
        assert_eq!(snake("UnitType"), "unit_type");
        assert_eq!(snake("AIModule"), "ai_module");
        assert_eq!(snake("UnitSizeType"), "unit_size_type");
        assert_eq!(camel("shift_queue_command"), "shiftQueueCommand");
        assert_eq!(camel("x"), "x");
        assert_eq!(c_scope("BWAPI::UnitTypes::Enum"), "UnitTypes");
        assert_eq!(c_scope("BWAPI::Text::Size::Enum"), "Size");
        assert_eq!(c_scope("BWAPI::UnitType"), "UnitType");
        assert_eq!(c_scope("BWAPI"), "BWAPI");
    }
}
