//! SHA-256 hashing encoded in Nix's custom base32 alphabet.
//!
//! Mirrors `printHash32` from nix/libutil: digits are little-endian 5-bit
//! groups over the byte array, printed most-significant group first, using
//! the alphabet `0123456789abcdfghijklmnpqrsvwxyz`.

use sha2::{Digest, Sha256};

const BASE32_CHARS: &[u8; 32] = b"0123456789abcdfghijklmnpqrsvwxyz";

/// Encode raw bytes in Nix base32.
pub fn nix_base32(bytes: &[u8]) -> String {
    assert!(!bytes.is_empty(), "cannot encode empty digest");
    let len = ((bytes.len() * 8 - 1) / 5) + 1;
    let mut out = String::with_capacity(len);
    for n in (0..len).rev() {
        let bit = n * 5;
        let byte = bit / 8;
        let shift = bit % 8;
        // u16 intermediate: `next << (8 - shift)` would overflow u8 when shift == 0.
        let lo = u16::from(bytes[byte]) >> shift;
        let hi = bytes
            .get(byte + 1)
            .map_or(0, |next| u16::from(*next) << (8 - shift));
        out.push(BASE32_CHARS[((lo | hi) & 0x1f) as usize] as char);
    }
    out
}

/// SHA-256 of `data`, encoded in Nix base32 (52 chars for SHA-256).
pub fn sha256_nix_base32(data: &[u8]) -> String {
    nix_base32(&Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_52_chars() {
        let h = sha256_nix_base32(b"");
        assert_eq!(h.len(), 52);
    }

    #[test]
    fn zero_hash_encodes_to_zeroes() {
        let zeros = [0u8; 32];
        assert_eq!(nix_base32(&zeros), "0".repeat(52));
    }

    #[test]
    fn deterministic_and_distinct() {
        let a = sha256_nix_base32(b"a");
        let b = sha256_nix_base32(b"b");
        assert_ne!(a, b);
        assert_eq!(a, sha256_nix_base32(b"a"));
    }
}
