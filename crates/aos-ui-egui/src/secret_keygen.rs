//! Local secret-string generator for the Settings vault UI.
//!
//! Generate fills a field only — Save remains an explicit user action so the
//! same value can be copied to a peer before vault write.

/// Alphabet used when sampling a secret string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum KeygenAlphabet {
    #[default]
    Hex,
    Numeric,
    Alphanumeric,
    Base64Url,
}

impl KeygenAlphabet {
    pub(crate) const ALL: [Self; 4] = [
        Self::Hex,
        Self::Numeric,
        Self::Alphanumeric,
        Self::Base64Url,
    ];

    pub(crate) fn charset(self) -> &'static [u8] {
        match self {
            Self::Hex => b"0123456789abcdef",
            Self::Numeric => b"0123456789",
            // Avoid ambiguous 0/O/1/I/l.
            Self::Alphanumeric => b"23456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz",
            Self::Base64Url => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
        }
    }
}

/// Which vault field receives a generated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum KeygenTarget {
    Brave,
    Github,
    Openai,
    #[default]
    LanSession,
}

impl KeygenTarget {
    pub(crate) const ALL: [Self; 4] = [
        Self::Brave,
        Self::Github,
        Self::Openai,
        Self::LanSession,
    ];
}

pub(crate) const KEYGEN_LEN_MIN: usize = 8;
pub(crate) const KEYGEN_LEN_MAX: usize = 256;
pub(crate) const LAN_SESSION_KEY_HEX_LEN: usize = 64;

/// True when `raw` is exactly 64 lowercase/uppercase hex digits (LAN session key).
pub(crate) fn is_valid_lan_session_key(raw: &str) -> bool {
    let raw = raw.trim();
    if raw.len() != LAN_SESSION_KEY_HEX_LEN {
        return false;
    }
    raw.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Sample `len` characters from `alphabet` using OS randomness.
pub(crate) fn generate(alphabet: KeygenAlphabet, len: usize) -> Result<String, String> {
    let len = len.clamp(KEYGEN_LEN_MIN, KEYGEN_LEN_MAX);
    let charset = alphabet.charset();
    if charset.is_empty() {
        return Err("empty charset".into());
    }
    let mut bytes = vec![0u8; len];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("getrandom: {e}"))?;
    let out: String = bytes
        .into_iter()
        .map(|b| {
            let idx = (b as usize) % charset.len();
            charset[idx] as char
        })
        .collect();
    Ok(out)
}

/// Apply the LAN preset: Hex alphabet and 64-character length.
pub(crate) fn apply_lan_preset(alphabet: &mut KeygenAlphabet, length: &mut usize) {
    *alphabet = KeygenAlphabet::Hex;
    *length = LAN_SESSION_KEY_HEX_LEN;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_respects_length_bounds() {
        let short = generate(KeygenAlphabet::Hex, 1).unwrap();
        assert_eq!(short.len(), KEYGEN_LEN_MIN);
        let long = generate(KeygenAlphabet::Hex, 10_000).unwrap();
        assert_eq!(long.len(), KEYGEN_LEN_MAX);
        let mid = generate(KeygenAlphabet::Numeric, 32).unwrap();
        assert_eq!(mid.len(), 32);
    }

    #[test]
    fn generate_hex_charset() {
        let s = generate(KeygenAlphabet::Hex, 64).unwrap();
        assert!(s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    }

    #[test]
    fn generate_numeric_charset() {
        let s = generate(KeygenAlphabet::Numeric, 24).unwrap();
        assert!(s.bytes().all(|b| b.is_ascii_digit()));
    }

    #[test]
    fn generate_alphanumeric_avoids_ambiguous() {
        let s = generate(KeygenAlphabet::Alphanumeric, 128).unwrap();
        assert!(!s.contains('0'));
        assert!(!s.contains('O'));
        assert!(!s.contains('1'));
        assert!(!s.contains('I'));
        assert!(!s.contains('l'));
    }

    #[test]
    fn generate_base64url_charset() {
        let allowed = KeygenAlphabet::Base64Url.charset();
        let s = generate(KeygenAlphabet::Base64Url, 48).unwrap();
        assert!(s.bytes().all(|b| allowed.contains(&b)));
    }

    #[test]
    fn lan_session_key_validation() {
        assert!(is_valid_lan_session_key(&"ab".repeat(32)));
        assert!(is_valid_lan_session_key(&"AB".repeat(32)));
        assert!(!is_valid_lan_session_key("pc-ai"));
        assert!(!is_valid_lan_session_key(&"ab".repeat(31)));
        assert!(!is_valid_lan_session_key(&"zz".repeat(32)));
        assert!(!is_valid_lan_session_key(""));
    }

    #[test]
    fn lan_preset_forces_hex_64() {
        let mut alphabet = KeygenAlphabet::Numeric;
        let mut length = 12;
        apply_lan_preset(&mut alphabet, &mut length);
        assert_eq!(alphabet, KeygenAlphabet::Hex);
        assert_eq!(length, 64);
    }
}
