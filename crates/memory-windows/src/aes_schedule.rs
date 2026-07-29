pub(crate) const AES_128_KEY_LEN: usize = 16;
pub(crate) const AES_128_SCHEDULE_LEN: usize = 176;
pub(crate) const AES_256_KEY_LEN: usize = 32;
pub(crate) const AES_256_SCHEDULE_LEN: usize = 240;

pub(crate) fn is_valid_aes_schedule(schedule: &[u8], key_len: usize) -> bool {
    let expected_len = match key_len {
        AES_128_KEY_LEN => AES_128_SCHEDULE_LEN,
        AES_256_KEY_LEN => AES_256_SCHEDULE_LEN,
        _ => return false,
    };
    if schedule.len() != expected_len {
        return false;
    }

    let mut generated = schedule[..key_len].to_vec();
    let mut rcon = 1u8;
    while generated.len() < expected_len {
        let mut word: [u8; 4] = generated[generated.len() - 4..]
            .try_into()
            .expect("AES word");
        if generated.len().is_multiple_of(key_len) {
            word.rotate_left(1);
            word.iter_mut().for_each(|byte| *byte = aes_sbox(*byte));
            word[0] ^= rcon;
            rcon = xtime(rcon);
        } else if key_len == AES_256_KEY_LEN && generated.len() % key_len == 16 {
            word.iter_mut().for_each(|byte| *byte = aes_sbox(*byte));
        }
        for byte in word {
            let next = generated[generated.len() - key_len] ^ byte;
            if schedule[generated.len()] != next {
                return false;
            }
            generated.push(next);
        }
    }
    true
}

fn aes_sbox(value: u8) -> u8 {
    let inverse = if value == 0 { 0 } else { gf_pow(value, 254) };
    inverse
        ^ inverse.rotate_left(1)
        ^ inverse.rotate_left(2)
        ^ inverse.rotate_left(3)
        ^ inverse.rotate_left(4)
        ^ 0x63
}

fn gf_pow(mut value: u8, mut exponent: u16) -> u8 {
    let mut result = 1u8;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = gf_mul(result, value);
        }
        value = gf_mul(value, value);
        exponent >>= 1;
    }
    result
}

fn gf_mul(mut left: u8, mut right: u8) -> u8 {
    let mut result = 0u8;
    for _ in 0..8 {
        if right & 1 != 0 {
            result ^= left;
        }
        left = xtime(left);
        right >>= 1;
    }
    result
}

fn xtime(value: u8) -> u8 {
    (value << 1) ^ if value & 0x80 != 0 { 0x1B } else { 0 }
}
