use blake3::Hasher;

pub fn cid_b3(bytes: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(bytes);
    let hash = h.finalize();
    format!("b3:{}", hex::encode(hash.as_bytes()))
}

/// Minimal hex helper (avoid extra dep usage elsewhere).
mod hex {
    pub fn encode(input: &[u8]) -> String {
        const LUT: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(input.len() * 2);
        for &b in input {
            out.push(LUT[(b >> 4) as usize] as char);
            out.push(LUT[(b & 0x0f) as usize] as char);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cid_len() {
        let c = cid_b3(b"hello");
        assert!(c.starts_with("b3:"));
        assert_eq!(c.len(), 2 + 1 + 64);
    }
}
