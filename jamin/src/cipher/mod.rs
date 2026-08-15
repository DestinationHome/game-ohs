pub mod constants;
pub mod hash;

pub struct CipherContext {
    cipher: Vec<u8>,
}

impl CipherContext {
    pub const fn new(cipher: Vec<u8>) -> Self {
        Self { cipher }
    }

    /// NOTE: This function is intentionally flawed.
    ///
    /// The original game's Lua code had a bug where it would wrap the index incorrectly
    /// when it exceeded the maximum index of the cipher array.
    ///
    /// We have to replicate this or we'll fail to decrypt OHS's payloads.
    const fn wrapped(index: usize, max: usize) -> usize {
        if index > max {
            1 + (index % (max + 1))
        } else {
            index
        }
    }

    fn get_cipher_byte(&self, index: usize) -> u8 {
        let max = self.cipher.len();

        let wrapped_idx = Self::wrapped(index, max);
        let final_idx = wrapped_idx.saturating_sub(1);

        self.cipher.get(final_idx).copied().unwrap_or(0)
    }

    pub fn decode(&self, encoded: &str) -> String {
        encoded.replace("&a", "&").replace("&m", "%")
    }

    pub fn encode(&self, plaintext: &str) -> String {
        plaintext.replace("&", "&a").replace("%", "&m")
    }

    pub fn decrypt(&self, encrypted: &str) -> Option<String> {
        let text = encrypted.trim_end_matches(['\n', '\r']);
        let bytes = text.as_bytes();

        ::tracing::debug!("Decrypting text: '{}'", text);

        if bytes.len() < 2 {
            return None;
        }

        let offsethi = bytes[0].checked_sub(32)? as usize;
        let offsetlo = bytes[1].checked_sub(32)? as usize;

        let offset = (offsethi * 95) + offsetlo + 1;

        let mut chars = Vec::with_capacity(bytes.len() - 2);

        for idx in 3..=(bytes.len()) {
            let srcbyte = match bytes[idx - 1].checked_sub(32) {
                Some(v) => v as i16,
                None => {
                    ::tracing::error!("Invalid character in input string at index {}", idx);
                    return None;
                }
            };

            let cipherbyte = self.get_cipher_byte(idx - 2 + offset) as i16;

            let decbyte = (srcbyte - cipherbyte).rem_euclid(95) as u8 + 32;
            chars.push(decbyte);
        }

        let result = String::from_utf8(chars);

        ::tracing::debug!("Decryption result: {:?}", result);

        result.ok()
    }

    pub fn encrypt(&self, plaintext: &str, offset: usize) -> Option<String> {
        if offset > 95 * 95 {
            return None;
        }

        ::tracing::debug!(
            "Encrypting plaintext: '{}' with offset: {}",
            plaintext,
            offset
        );

        let text_bytes = plaintext.as_bytes();
        let mut chars = Vec::with_capacity(text_bytes.len() + 2);

        // Header math remains the same
        let offsethi = ((offset - 1) / 95) as u8;
        let offsetlo = ((offset - 1) % 95) as u8;

        chars.push(offsethi + 32);
        chars.push(offsetlo + 32);

        for idx in 3..=(text_bytes.len() + 2) {
            let srcbyte = match text_bytes[idx - 3].checked_sub(32) {
                Some(v) if v <= 94 => v as i16,
                _ => return None,
            };

            let cipherbyte = self.get_cipher_byte(idx - 2 + offset) as i16;

            // Standard forward addition wrapper
            let encbyte = (srcbyte + cipherbyte).rem_euclid(95) as u8 + 32;
            chars.push(encbyte);
        }

        let result = String::from_utf8(chars);

        ::tracing::debug!("Encryption result: {:?}", result);

        result.ok()
    }
}
