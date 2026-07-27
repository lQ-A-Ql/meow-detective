use super::*;

/// The 512-byte pattern the upstream regression vector was captured against.
fn sample_sector() -> Vec<u8> {
    (0..512u32)
        .map(|index| (index.wrapping_mul(31) ^ 0xA5) as u8)
        .collect()
}

/// Sector key `0x00..=0x1f`, matching the upstream capture.
fn sample_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = index as u8;
    }
    key
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[test]
fn decrypt_matches_the_upstream_regression_vector() {
    // Captured from the oracle-validated upstream before vendoring. This is the
    // only test here that can catch a transposed rotation constant or a swapped
    // cycle order — a round-trip cannot, because both directions would move
    // together. The authoritative proof remains the public bdetogo.raw oracle.
    let mut sector = sample_sector();
    decrypt(&mut sector, &sample_key());
    assert_eq!(
        hex(&sector[..32]),
        "9649e3f15c8ecdb6fceb5a864f24e97596052689bf414d5c3137edb27dc43c6e"
    );
    assert_eq!(hex(&sector[496..512]), "21eafdd00ad4826068a2d7a8f28fcf97");
}

#[test]
fn decrypt_uses_every_byte_of_the_sector_key() {
    // The XOR stage walks the key modulo 32, so a change in any key byte must
    // reach the output. A key used only partially would leave bytes fixed.
    let baseline = {
        let mut sector = sample_sector();
        decrypt(&mut sector, &sample_key());
        sector
    };
    for index in 0..32usize {
        let mut key = sample_key();
        key[index] ^= 0xFF;
        let mut sector = sample_sector();
        decrypt(&mut sector, &key);
        assert_ne!(
            sector, baseline,
            "key byte {index} did not reach the output"
        );
    }
}

#[test]
fn decrypt_diffuses_one_input_byte_across_the_sector() {
    // Avalanche is the whole point: flipping one input byte must disturb most of
    // the sector, not just its own word.
    let key = sample_key();
    let baseline = {
        let mut sector = sample_sector();
        decrypt(&mut sector, &key);
        sector
    };
    let mut perturbed = sample_sector();
    perturbed[0] ^= 0x01;
    decrypt(&mut perturbed, &key);

    let differing = baseline
        .iter()
        .zip(perturbed.iter())
        .filter(|(left, right)| left != right)
        .count();
    assert!(
        differing > 400,
        "one flipped input byte only changed {differing} of 512 output bytes"
    );
}

#[test]
fn empty_and_sub_word_inputs_do_not_panic() {
    let key = sample_key();
    let mut empty: [u8; 0] = [];
    decrypt(&mut empty, &key);
    // Below one word `chunks_exact` yields nothing, so only the XOR stage runs.
    // The sample key is 0x00,0x01,0x02,... so the result is the input XOR its index.
    let mut three = [1u8, 2, 3];
    decrypt(&mut three, &key);
    assert_eq!(three, [1, 3, 1]);
}

#[test]
fn tiny_word_counts_do_not_panic() {
    // Real sectors are 128 words, so the diffuser is only ever driven at
    // count >= 5. A naive `index + count - 5` underflows for 1..=4 words, which a
    // fuzzer over arbitrary bytes would reach.
    let key = sample_key();
    for words in 1..=6usize {
        let mut sector = vec![0xABu8; words * 4];
        decrypt(&mut sector, &key);
    }
}

#[test]
fn to_words_and_from_words_round_trip() {
    let sector = sample_sector();
    let words = to_words(&sector);
    assert_eq!(words.len(), 128);
    let mut out = vec![0u8; sector.len()];
    from_words(&words, &mut out);
    assert_eq!(out, sector);
}

#[test]
fn to_words_drops_a_partial_trailing_word() {
    // Documents why the XOR stage has to cover bytes the word stages do not.
    assert_eq!(to_words(&[1u8, 0, 0, 0, 9, 9]).len(), 1);
}

#[test]
fn from_words_ignores_words_past_the_output() {
    let mut out = [0u8; 4];
    from_words(&[0x0403_0201, 0xDEAD_BEEF], &mut out);
    assert_eq!(out, [1, 2, 3, 4]);
}

#[test]
fn wrapping_back_stays_in_range() {
    for count in 1..=8usize {
        for index in 0..count {
            for back in [0usize, 2, 5, 100] {
                let resolved = wrapping_back(index, back, count);
                assert!(
                    resolved < count,
                    "wrapping_back({index}, {back}, {count}) = {resolved}"
                );
            }
        }
    }
}

#[test]
fn wrapping_back_matches_the_reference_form_for_real_sectors() {
    // For count >= 5 this must equal the reference `(i + n - k) % n`.
    for count in 5..=130usize {
        for index in 0..count {
            for back in [2usize, 5] {
                assert_eq!(
                    wrapping_back(index, back, count),
                    (index + count - back) % count
                );
            }
        }
    }
}
