//! Token boundaries need not coincide with UTF-8 character boundaries.

#[derive(Default)]
pub(crate) struct StreamingUtf8 {
    pending: Vec<u8>,
}

impl StreamingUtf8 {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(text) => {
                    output.push_str(text);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid_end = consumed + error.valid_up_to();
                    output
                        .push_str(std::str::from_utf8(&self.pending[consumed..valid_end]).unwrap());
                    consumed = valid_end;
                    match error.error_len() {
                        Some(invalid_len) => {
                            output.push('\u{fffd}');
                            consumed += invalid_len;
                        }
                        None => break, // Keep an incomplete character for the next token.
                    }
                }
            }
        }
        self.pending.drain(..consumed);
        output
    }

    /// A completed response may end inside a character (e.g. max_tokens).
    pub(crate) fn finish(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_boundaries_preserve_multilingual_text() {
        let text = "été 猫 🧭";
        for split in 0..=text.len() {
            let mut decoder = StreamingUtf8::default();
            let mut output = decoder.push(&text.as_bytes()[..split]);
            assert!(!output.contains('\u{fffd}'));
            output.push_str(&decoder.push(&text.as_bytes()[split..]));
            output.push_str(&decoder.finish());
            assert_eq!(output, text, "split at byte {split}");
        }
    }

    #[test]
    fn interleaved_streams_keep_their_own_partial_character() {
        let mut compass = StreamingUtf8::default();
        let mut cat = StreamingUtf8::default();
        assert_eq!(compass.push(&[0xf0, 0x9f]), "");
        assert_eq!(cat.push(&[0xe7]), "");
        assert_eq!(compass.push(&[0xa7]), "");
        assert_eq!(cat.push(&[0x8c, 0xab]), "猫");
        assert_eq!(compass.push(&[0xad]), "🧭");
        assert_eq!(compass.finish(), "");
        assert_eq!(cat.finish(), "");
    }

    #[test]
    fn invalid_and_truncated_bytes_match_whole_response_lossy_decoding() {
        for bytes in [b"a\xffb".as_slice(), b"\xe2\x82a", b"\xf0\x9f", b"\xc0\xaf"] {
            for split in 0..=bytes.len() {
                let mut decoder = StreamingUtf8::default();
                let mut output = decoder.push(&bytes[..split]);
                output.push_str(&decoder.push(&bytes[split..]));
                output.push_str(&decoder.finish());
                assert_eq!(output, String::from_utf8_lossy(bytes));
                assert_eq!(decoder.finish(), "");
            }
        }
    }
}
