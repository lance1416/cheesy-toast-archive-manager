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
    fn test_pure_utf8() {
        let raw = "Hello, World!".as_bytes();
        let (decoded, enc) = decode_bytes(raw, None);

        assert_eq!(decoded, "Hello, World!");
        assert_eq!(enc, "UTF-8");
    }

    #[test]
    fn test_japanese_shift_jis() {
        // "テスト" (Test) encoded in Shift-JIS
        let raw_sjis: [u8; 6] = [0x83, 0x65, 0x83, 0x58, 0x83, 0x67];
        let (decoded, enc) = decode_bytes(&raw_sjis, None);

        assert_eq!(decoded, "テスト");
        // chardetng usually maps Shift-JIS to windows-31j (which is the Microsoft extension of Shift-JIS)
        assert_eq!(enc, "Shift_JIS");
    }

    #[test]
    fn test_chinese_gbk() {
        // "测试" (Test) encoded in GBK
        let raw_gbk: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4];
        let (decoded, enc) = decode_bytes(&raw_gbk, None);

        assert_eq!(decoded, "测试");
        assert_eq!(enc, "GBK");
    }

    #[test]
    fn test_forced_encoding() {
        // "测试" in GBK, but we'll deliberately force Shift-JIS
        let raw_gbk: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4];
        let (decoded, enc) = decode_bytes(&raw_gbk, Some("Shift_JIS"));

        // It will decode to gibberish (which is expected when forcing the wrong encoding),
        // but it proves the UI override intercepts the flow.
        assert_ne!(decoded, "测试");
        assert_eq!(enc, "Shift_JIS");
    }
}
