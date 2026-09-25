//! Idempotency keys.
//!
//! A ULID: 48 bits of milliseconds and 80 random bits, in Crockford base 32.
//! Sortable by time, which makes a platform's log of keys readable, and random
//! enough that two attempts from any number of frames never share one.

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A key from a time and ten random bytes.
pub(crate) fn ulid(millis: u64, random: [u8; 10]) -> String {
    let mut value = u128::from(millis & 0xFFFF_FFFF_FFFF) << 80;
    for (index, byte) in random.iter().enumerate() {
        value |= u128::from(*byte) << (72 - 8 * index);
    }
    (0..26)
        .map(|index| {
            let shift = 125 - 5 * index;
            ALPHABET[((value >> shift) & 31) as usize] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_twenty_six_crockford_characters() {
        let key = ulid(1_790_200_000_000, [0xAB; 10]);
        assert_eq!(key.len(), 26);
        assert!(key.bytes().all(|byte| ALPHABET.contains(&byte)));
    }

    #[test]
    fn keys_sort_by_time() {
        let earlier = ulid(1_000, [0xFF; 10]);
        let later = ulid(1_001, [0x00; 10]);
        assert!(earlier < later);
    }

    #[test]
    fn the_same_time_with_different_randomness_is_a_different_key() {
        assert_ne!(ulid(5, [1; 10]), ulid(5, [2; 10]));
    }
}
