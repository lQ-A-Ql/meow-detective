pub(crate) fn is_newer(candidate: &str, reference: &str) -> bool {
    let candidate = parse_parts(candidate);
    let reference = parse_parts(reference);
    let len = candidate.len().max(reference.len());
    for index in 0..len {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = reference.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false
}

fn parse_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}
