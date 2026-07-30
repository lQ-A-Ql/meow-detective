use std::fmt::Write;

use crate::RecoveryPassword;

use super::material::RecoveryPasswordMaterial;

pub(super) fn format_material(material: &RecoveryPasswordMaterial) -> RecoveryPassword {
    let mut output = String::with_capacity(55);
    for (index, bytes) in material.expose_for_formatting().chunks_exact(2).enumerate() {
        if index != 0 {
            output.push('-');
        }
        let word = u16::from_le_bytes([bytes[0], bytes[1]]);
        let group = u32::from(word) * 11;
        let _ = write!(output, "{group:06}");
    }
    RecoveryPassword::from_formatted(output)
}
