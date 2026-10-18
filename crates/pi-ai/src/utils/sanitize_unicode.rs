/// Removes unpaired UTF-16 surrogates, matching the TypeScript regex.
///
/// Rust `str` values are valid Unicode and cannot contain unpaired surrogates,
/// so the string API is an identity function. Callers holding raw UTF-16 code
/// units use [`sanitize_surrogate_units`].
pub fn sanitize_surrogates(text: &str) -> &str {
    text
}

pub fn sanitize_surrogate_units(units: &[u16]) -> Vec<u16> {
    let mut sanitized = Vec::with_capacity(units.len());

    for (index, unit) in units.iter().copied().enumerate() {
        match unit {
            0xd800..=0xdbff => {
                if units.get(index + 1).is_some_and(|next| is_low_surrogate(*next)) {
                    sanitized.push(unit);
                }
            }
            0xdc00..=0xdfff => {
                if index > 0 && is_high_surrogate(units[index - 1]) {
                    sanitized.push(unit);
                }
            }
            _ => sanitized.push(unit),
        }
    }

    sanitized
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xd800..=0xdbff).contains(&unit)
}

fn is_low_surrogate(unit: u16) -> bool {
    (0xdc00..=0xdfff).contains(&unit)
}
