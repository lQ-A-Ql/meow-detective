use super::ExtractionSchedulingPolicy;

pub(super) fn reject_oversized<E>(
    weight: usize,
    policy: ExtractionSchedulingPolicy,
    sequence: usize,
    error: &impl Fn(usize, String) -> Option<E>,
) -> Result<bool, E> {
    if weight <= policy.max_in_flight_bytes.max(1) {
        return Ok(false);
    }
    let message = format!(
        "analysis candidate requires {weight} bytes, exceeding the {} byte input budget",
        policy.max_in_flight_bytes,
    );
    match error(sequence, message) {
        Some(error) => Err(error),
        None => Ok(true),
    }
}
