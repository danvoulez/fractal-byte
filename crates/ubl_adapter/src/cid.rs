/// Compute a BLAKE3 content-addressed ID for arbitrary bytes.
pub fn cid_b3(bytes: &[u8]) -> String {
    let hash = blake3::hash(bytes);
    format!("b3:{}", hex::encode(hash.as_bytes()))
}
