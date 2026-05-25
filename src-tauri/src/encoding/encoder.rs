use encoding_rs::Encoding;

/// Encodes a UTF-8 string into raw bytes targeting a specific legacy encoding.
pub fn encode_string(text: &str, target_encoding: Option<&str>) -> Result<Vec<u8>, String> {
    if let Some(label) = target_encoding {
        return if let Some(encoding) = Encoding::for_label(label.as_bytes()) {
            let (encoded_bytes, _, unmappable) = encoding.encode(text);

            // text contains characters not exist in the target legacy encoding
            if unmappable {
                return Err(format!(
                    "The text contains characters not supported by the '{}' encoding.",
                    encoding.name()
                ));
            }

            Ok(encoded_bytes.into_owned())
        } else {
            Err(format!("Unsupported encoding label: {}", label))
        };
    }

    // Fallback to UTF-8
    Ok(text.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_hint_returns_utf8_bytes() {
        assert_eq!(
            encode_string("Password123!", None).unwrap(),
            b"Password123!"
        );
    }

    #[test]
    fn gbk_hint_encodes_chinese_correctly() {
        let expected: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4]; // GBK for "测试"
        assert_eq!(encode_string("测试", Some("GBK")).unwrap(), expected);
    }

    #[test]
    fn shift_jis_hint_encodes_japanese_correctly() {
        let expected: [u8; 6] = [0x83, 0x65, 0x83, 0x58, 0x83, 0x67]; // Shift-JIS for "テスト"
        assert_eq!(
            encode_string("テスト", Some("Shift_JIS")).unwrap(),
            expected
        );
    }

    #[test]
    fn unmappable_character_returns_err() {
        // Emoji postdates GBK — cannot be represented
        let result = encode_string("测试🚀", Some("GBK"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }

    #[test]
    fn invalid_encoding_label_returns_err() {
        let result = encode_string("test", Some("not-a-real-encoding"));
        assert!(result.is_err());
    }
}
