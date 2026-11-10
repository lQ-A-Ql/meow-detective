use super::{parity_placement, Placement};

const GF_POLYNOMIAL: u8 = 0x1d;

#[allow(clippy::too_many_arguments)]
pub(super) fn rebuild(
    level: i32,
    layout: u32,
    n: usize,
    data_disks: usize,
    row: u64,
    target: Placement,
    columns: &[Option<Vec<u8>>],
    output: &mut [u8],
) -> std::io::Result<bool> {
    let data = data_columns(level, layout, n, data_disks, row, columns);
    let missing = data
        .iter()
        .enumerate()
        .filter_map(|(position, bytes)| bytes.is_none().then_some(position))
        .collect::<Vec<_>>();
    match missing.as_slice() {
        [] | [_] => rebuild_one(level, layout, n, row, target, &data, columns, output),
        [first, second] => rebuild_two(
            level, layout, n, row, target, &data, columns, *first, *second, output,
        ),
        _ => Ok(false),
    }
}

fn data_columns(
    level: i32,
    layout: u32,
    n: usize,
    data_disks: usize,
    row: u64,
    columns: &[Option<Vec<u8>>],
) -> Vec<Option<Vec<u8>>> {
    (0..data_disks)
        .map(|position| {
            let placement = parity_placement(level, layout, n, row, position);
            columns.get(placement.column).and_then(Clone::clone)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn rebuild_one(
    level: i32,
    layout: u32,
    n: usize,
    row: u64,
    target: Placement,
    data: &[Option<Vec<u8>>],
    columns: &[Option<Vec<u8>>],
    output: &mut [u8],
) -> std::io::Result<bool> {
    let Some(data_position) = (0..data.len()).find(|position| {
        parity_placement(level, layout, n, row, *position).column == target.column
    }) else {
        return Ok(false);
    };
    if let Some(value) = &data[data_position] {
        output.copy_from_slice(value);
        return Ok(true);
    }
    if let Some(parity) = columns.get(target.p).and_then(Clone::clone) {
        let mut value = parity;
        for (position, bytes) in data.iter().enumerate() {
            if position != data_position {
                if let Some(bytes) = bytes {
                    xor_assign(&mut value, bytes);
                }
            }
        }
        output.copy_from_slice(&value);
        return Ok(true);
    }
    if level != 6 {
        return Ok(false);
    }
    let Some(q) = target.q.and_then(|q| columns.get(q)).and_then(Clone::clone) else {
        return Ok(false);
    };
    let exponent = parity_placement(level, layout, n, row, data_position).q_exponent;
    let mut syndrome = q;
    for (position, bytes) in data.iter().enumerate() {
        if position != data_position {
            if let Some(bytes) = bytes {
                let exponent = parity_placement(level, layout, n, row, position).q_exponent;
                xor_assign(&mut syndrome, &mul_bytes(bytes, gf_pow(exponent)));
            }
        }
    }
    output.copy_from_slice(&div_bytes(&syndrome, gf_pow(exponent)));
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn rebuild_two(
    level: i32,
    layout: u32,
    n: usize,
    row: u64,
    target: Placement,
    data: &[Option<Vec<u8>>],
    columns: &[Option<Vec<u8>>],
    first: usize,
    second: usize,
    output: &mut [u8],
) -> std::io::Result<bool> {
    if level != 6 {
        return Ok(false);
    }
    let Some(mut p_syndrome) = columns.get(target.p).and_then(Clone::clone) else {
        return Ok(false);
    };
    let Some(mut q_syndrome) = target.q.and_then(|q| columns.get(q)).and_then(Clone::clone) else {
        return Ok(false);
    };
    for (position, bytes) in data.iter().enumerate() {
        if position == first || position == second {
            continue;
        }
        if let Some(bytes) = bytes {
            xor_assign(&mut p_syndrome, bytes);
            let exponent = parity_placement(level, layout, n, row, position).q_exponent;
            xor_assign(&mut q_syndrome, &mul_bytes(bytes, gf_pow(exponent)));
        }
    }
    let first_placement = parity_placement(level, layout, n, row, first);
    let second_placement = parity_placement(level, layout, n, row, second);
    let first_coefficient = gf_pow(first_placement.q_exponent);
    let second_coefficient = gf_pow(second_placement.q_exponent);
    let first_value = div_bytes(
        &xor_bytes(&q_syndrome, &mul_bytes(&p_syndrome, second_coefficient)),
        first_coefficient ^ second_coefficient,
    );
    let second_value = xor_bytes(&p_syndrome, &first_value);
    let selected = if target.column == first_placement.column {
        &first_value
    } else if target.column == second_placement.column {
        &second_value
    } else {
        return Ok(false);
    };
    output.copy_from_slice(selected);
    Ok(true)
}

fn xor_assign(target: &mut [u8], source: &[u8]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target ^= source;
    }
}

fn xor_bytes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut output = left.to_vec();
    xor_assign(&mut output, right);
    output
}

fn mul_bytes(bytes: &[u8], coefficient: u8) -> Vec<u8> {
    bytes
        .iter()
        .map(|byte| gf_mul(*byte, coefficient))
        .collect()
}

fn div_bytes(bytes: &[u8], coefficient: u8) -> Vec<u8> {
    let inverse = gf_inv(coefficient);
    bytes.iter().map(|byte| gf_mul(*byte, inverse)).collect()
}

fn gf_mul(mut left: u8, mut right: u8) -> u8 {
    let mut product = 0u8;
    for _ in 0..8 {
        if right & 1 != 0 {
            product ^= left;
        }
        let high = left & 0x80;
        left <<= 1;
        if high != 0 {
            left ^= GF_POLYNOMIAL;
        }
        right >>= 1;
    }
    product
}

fn gf_pow(mut exponent: usize) -> u8 {
    let mut value = 1u8;
    let mut base = 2u8;
    while exponent > 0 {
        if exponent & 1 != 0 {
            value = gf_mul(value, base);
        }
        base = gf_mul(base, base);
        exponent >>= 1;
    }
    value
}

fn gf_inv(value: u8) -> u8 {
    let mut result = 1u8;
    for _ in 0..254 {
        result = gf_mul(result, value);
    }
    result
}
