use crate::registry::lookup::RegistryHiveReader;

const BOOT_KEY_PERMUTATION: [usize; 16] = [8, 5, 4, 2, 11, 9, 13, 3, 0, 6, 1, 12, 14, 10, 15, 7];

pub fn extract_boot_key(system_hive: &[u8]) -> Option<[u8; 16]> {
    let hive = RegistryHiveReader::new(system_hive).ok()?;
    let control_sets = hive.control_set_candidates(&mut Vec::new());

    for cs in control_sets {
        let lsa_path: &[&str] = &[cs.as_str(), "Control", "LSA"];
        let lsa_nk = hive.navigate_to(lsa_path).ok()??;
        let subkey_names = hive.read_subkey_names_from_nk(&lsa_nk).ok()?;
        let mut hex_combined = String::new();

        for subkey_name in ["JD", "Skew1", "GBG", "Data"] {
            if !subkey_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(subkey_name))
            {
                hex_combined.clear();
                break;
            }
            let mut path = lsa_path.to_vec();
            path.push(subkey_name);
            hex_combined.push_str(&hive.read_class_name_at(&path).ok()??);
        }

        if hex_combined.is_empty() {
            continue;
        }
        let scrambled = decode_hex_class_name(&hex_combined)?;
        if scrambled.len() < BOOT_KEY_PERMUTATION.len() {
            continue;
        }

        let mut boot_key = [0u8; 16];
        for (index, source) in BOOT_KEY_PERMUTATION.iter().enumerate() {
            boot_key[index] = *scrambled.get(*source)?;
        }
        return Some(boot_key);
    }

    None
}

fn decode_hex_class_name(raw: &str) -> Option<Vec<u8>> {
    let cleaned: String = raw
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ',')
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return None;
    }
    cleaned
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_char_to_nibble(pair[0])?;
            let low = hex_char_to_nibble(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_char_to_nibble(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
}
