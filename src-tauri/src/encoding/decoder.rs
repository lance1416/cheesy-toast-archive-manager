use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;

/// Decodes raw bytes into a UTF-8 String, returning the decoded string and the encoding used.
pub fn decode_bytes(raw: &[u8], encoding: Option<&str>) -> (String, String) {
    if let Some(label) = encoding {
        if let Some(enc) = Encoding::for_label(label.as_bytes()) {
            let (decoded, _, _) = enc.decode(raw);
            return (decoded.into_owned(), enc.name().to_string());
        }
    }

    // Try UTF-8
    if let Ok(utf8_str) = std::str::from_utf8(raw) {
        return (utf8_str.to_string(), "UTF-8".to_string());
    }

    // Fallback to heuristic guessing for legacy encodings
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(raw, true);

    // Guess the encoding
    let guessed_encoding = detector.guess(None, Utf8Detection::Allow);
    let (decoded, _, _) = guessed_encoding.decode(raw);

    (decoded.into_owned(), guessed_encoding.name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bytes_decoded_and_reported_as_utf8() {
        let (s, enc) = decode_bytes(b"Hello, World!", None);
        assert_eq!(s, "Hello, World!");
        assert_eq!(enc, "UTF-8");
    }

    #[test]
    fn gbk_bytes_auto_detected_heuristically() {
        let raw: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4]; // "测试" in GBK
        let (s, enc) = decode_bytes(&raw, None);
        assert_eq!(s, "测试");
        assert_eq!(enc, "GBK");
    }

    #[test]
    fn shift_jis_bytes_auto_detected_heuristically() {
        let raw: [u8; 6] = [0x83, 0x65, 0x83, 0x58, 0x83, 0x67]; // "テスト" in Shift-JIS
        let (s, enc) = decode_bytes(&raw, None);
        assert_eq!(s, "テスト");
        assert_eq!(enc, "Shift_JIS");
    }

    #[test]
    fn explicit_hint_decodes_correctly_and_is_recorded() {
        let raw: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4]; // "测试" in GBK
        let (s, enc) = decode_bytes(&raw, Some("GBK"));
        assert_eq!(s, "测试");
        assert_eq!(enc, "GBK");
    }

    #[test]
    fn explicit_hint_overrides_heuristic_even_when_wrong() {
        // GBK bytes forced through Shift-JIS → garbled output, but Shift_JIS is recorded
        let raw: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4];
        let (s, enc) = decode_bytes(&raw, Some("Shift_JIS"));
        assert_ne!(s, "测试");
        assert_eq!(enc, "Shift_JIS");
    }

    #[test]
    fn unknown_encoding_label_falls_through_to_heuristic() {
        // An unrecognised label is silently ignored; chardetng still picks up GBK
        let raw: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4]; // "测试" in GBK
        let (s, enc) = decode_bytes(&raw, Some("not-a-real-encoding"));
        assert_eq!(s, "测试");
        assert_eq!(enc, "GBK");
    }
}
