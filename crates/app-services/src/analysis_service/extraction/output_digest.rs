use domain::{Artifact, TimelineEvent};
use sha2::{Digest, Sha256};

const DIGEST_DOMAIN: &[u8] = b"analysis-output-v3";
const ARTIFACT_ITEM_DOMAIN_A: &[u8] = b"analysis-artifact-item-v3:a";
const ARTIFACT_ITEM_DOMAIN_B: &[u8] = b"analysis-artifact-item-v3:b";
const TIMELINE_ITEM_DOMAIN_A: &[u8] = b"analysis-timeline-item-v3:a";
const TIMELINE_ITEM_DOMAIN_B: &[u8] = b"analysis-timeline-item-v3:b";

#[derive(Default)]
pub(super) struct OutputDigestAccumulator {
    artifacts: MultisetAccumulator,
    timeline_events: MultisetAccumulator,
}

impl OutputDigestAccumulator {
    pub(super) fn record_artifact(&mut self, artifact: &Artifact) {
        self.artifacts.record(
            &artifact_digest_record(artifact),
            ARTIFACT_ITEM_DOMAIN_A,
            ARTIFACT_ITEM_DOMAIN_B,
        );
    }

    pub(super) fn record_timeline_event(&mut self, event: &TimelineEvent) {
        self.timeline_events.record(
            &timeline_digest_record(event),
            TIMELINE_ITEM_DOMAIN_A,
            TIMELINE_ITEM_DOMAIN_B,
        );
    }

    pub(super) fn finish(self) -> String {
        let mut hasher = Sha256::new();
        update_digest_field(&mut hasher, DIGEST_DOMAIN);
        self.artifacts.finish_into(&mut hasher);
        self.timeline_events.finish_into(&mut hasher);
        hex::encode(hasher.finalize())
    }
}

pub(super) fn output_digest_for_outputs<'a>(
    artifacts: impl IntoIterator<Item = &'a Artifact>,
    timeline_events: impl IntoIterator<Item = &'a TimelineEvent>,
) -> String {
    let mut accumulator = OutputDigestAccumulator::default();
    for artifact in artifacts {
        accumulator.record_artifact(artifact);
    }
    for event in timeline_events {
        accumulator.record_timeline_event(event);
    }
    accumulator.finish()
}

#[derive(Default)]
struct MultisetAccumulator {
    count: u64,
    sum_a: [u8; 32],
    sum_b: [u8; 32],
}

impl MultisetAccumulator {
    fn record(&mut self, record: &[u8], domain_a: &[u8], domain_b: &[u8]) {
        self.count = self.count.saturating_add(1);
        add_digest(&mut self.sum_a, item_digest(domain_a, record));
        add_digest(&mut self.sum_b, item_digest(domain_b, record));
    }

    fn finish_into(self, hasher: &mut Sha256) {
        hasher.update(self.count.to_le_bytes());
        hasher.update(self.sum_a);
        hasher.update(self.sum_b);
    }
}

fn item_digest(domain: &[u8], record: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    update_digest_field(&mut hasher, domain);
    update_digest_field(&mut hasher, record);
    hasher.finalize().into()
}

fn add_digest(accumulator: &mut [u8; 32], value: [u8; 32]) {
    let mut carry = 0u16;
    for (target, addend) in accumulator.iter_mut().zip(value).rev() {
        let sum = u16::from(*target) + u16::from(addend) + carry;
        *target = sum as u8;
        carry = sum >> 8;
    }
}

fn artifact_digest_record(artifact: &Artifact) -> Vec<u8> {
    let mut record = Vec::new();
    append_record_field(&mut record, artifact.family.as_bytes());
    append_optional_record_field(
        &mut record,
        artifact.source_object_id.as_ref().map(|id| id.0.as_bytes()),
    );
    append_optional_record_field(
        &mut record,
        artifact.extractor_id.as_deref().map(str::as_bytes),
    );
    append_optional_record_field(
        &mut record,
        artifact.extractor_version.as_deref().map(str::as_bytes),
    );
    append_optional_f32(&mut record, artifact.confidence);
    append_optional_record_field(
        &mut record,
        artifact.source_attribution.as_deref().map(str::as_bytes),
    );
    append_record_field(&mut record, artifact.title.as_bytes());
    append_record_field(&mut record, artifact.summary.as_bytes());
    append_record_field(
        &mut record,
        serde_json::to_string(&artifact.attrs)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    );
    record
}

fn timeline_digest_record(event: &TimelineEvent) -> Vec<u8> {
    let mut record = Vec::new();
    append_record_field(&mut record, event.source_object_id.as_bytes());
    append_record_field(&mut record, event.event_type.as_bytes());
    append_record_field(&mut record, event.timestamp.to_rfc3339().as_bytes());
    append_record_field(&mut record, event.title.as_bytes());
    append_record_field(&mut record, event.description.as_bytes());
    append_optional_record_field(&mut record, event.parser_id.as_deref().map(str::as_bytes));
    append_optional_record_field(
        &mut record,
        event.parser_version.as_deref().map(str::as_bytes),
    );
    append_optional_f32(&mut record, event.confidence);
    append_optional_record_field(
        &mut record,
        event.source_attribution.as_deref().map(str::as_bytes),
    );
    append_record_field(
        &mut record,
        serde_json::to_string(&event.attrs)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    );
    record
}

fn append_optional_record_field(record: &mut Vec<u8>, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            record.push(1);
            append_record_field(record, value);
        }
        None => record.push(0),
    }
}

fn append_optional_f32(record: &mut Vec<u8>, value: Option<f32>) {
    match value {
        Some(value) => {
            record.push(1);
            record.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        None => record.push(0),
    }
}

fn append_record_field(record: &mut Vec<u8>, value: &[u8]) {
    record.extend_from_slice(&(value.len() as u64).to_le_bytes());
    record.extend_from_slice(value);
}

fn update_digest_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
