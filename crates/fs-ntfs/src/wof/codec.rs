use super::{format::WofAlgorithm, lzx, xpress, Result, WofError};

pub(super) fn decompress(data: &[u8], size: usize, algorithm: WofAlgorithm) -> Result<Vec<u8>> {
    if size == 0 || size as u64 > algorithm.chunk_size() || data.is_empty() || data.len() > size {
        return Err(WofError::Invalid("invalid stored or logical chunk size"));
    }
    // Chunks that did not compress are stored verbatim without codec framing.
    if data.len() == size {
        return Ok(data.to_vec());
    }
    let output = match algorithm {
        WofAlgorithm::Lzx => lzx::decompress(data, size)?,
        _ => xpress::decompress(data, size)?,
    };
    if output.len() != size {
        return Err(WofError::Invalid(
            "decoded chunk length differs from metadata",
        ));
    }
    Ok(output)
}

pub(super) fn copy_match(
    output: &mut Vec<u8>,
    distance: usize,
    length: usize,
    end: usize,
) -> Result<()> {
    if distance == 0 || distance > output.len() || length > end.saturating_sub(output.len()) {
        return Err(WofError::Invalid("match lies outside the decoded chunk"));
    }
    for _ in 0..length {
        let byte = output[output.len() - distance];
        output.push(byte);
    }
    Ok(())
}
