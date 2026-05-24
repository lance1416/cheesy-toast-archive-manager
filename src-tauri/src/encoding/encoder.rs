use encoding_rs::Encoding;

/// Encodes a UTF-8 string into raw bytes targeting a specific legacy encoding.
#[allow(dead_code)]
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
    fn test_encode_pure_utf8() {
        let text = "Password123!";
        let encoded = encode_string(text, None).unwrap();

        assert_eq!(encoded, text.as_bytes());
    }

    #[test]
    fn test_encode_japanese_shift_jis() {
        let text = "テスト"; // "Test"
        let encoded = encode_string(text, Some("Shift_JIS")).unwrap();

        // Shift-JIS byte sequence for "テスト"
        let expected: [u8; 6] = [0x83, 0x65, 0x83, 0x58, 0x83, 0x67];
        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_encode_chinese_gbk() {
        let text = "测试"; // "Test"
        let encoded = encode_string(text, Some("GBK")).unwrap();

        // GBK byte sequence for "测试"
        let expected: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4];
        assert_eq!(encoded, expected);
    }

    #[test]
    fn test_encode_unmappable_character() {
        // "测试" combined with a modern Emoji that didn't exist in 1990s GBK
        let text = "测试🚀";
        let result = encode_string(text, Some("GBK"));

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not supported"));
    }
}
