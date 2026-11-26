use super::fields::{category, selected, system_fields, text};
use crate::analysis_service::AnalysisServiceError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use transport::dto::{ReportEntityDto, ReportSectionDto, ReportSourceSummaryDto};

const MAX_ENTITIES: usize = 256;

#[derive(Default)]
pub(super) struct Summary {
    sections: [Section; 4],
}

#[derive(Default)]
struct Section {
    records: u64,
    partial: bool,
    entities: BTreeMap<String, ReportEntityDto>,
    counts: BTreeMap<String, u64>,
}

impl Summary {
    pub(super) fn push(
        &mut self,
        family: &str,
        payload: &Value,
    ) -> Result<(), AnalysisServiceError> {
        let attrs = &payload["attrs"];
        let Some((index, identity, names)) = category(family, attrs) else {
            return Ok(());
        };
        let section = &mut self.sections[index];
        section.records += 1;
        increment(&mut section.counts, family, 1)?;
        let attachments = attrs
            .get("attachmentCount")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if family == "EmailMessage" {
            increment(&mut section.counts, "attachments", attachments)?;
        }
        let identity = hex::encode(Sha256::digest(identity.as_bytes()));
        if section.entities.len() >= MAX_ENTITIES && !section.entities.contains_key(&identity) {
            section.partial = true;
            return Ok(());
        }
        let entity = section
            .entities
            .entry(identity.clone())
            .or_insert_with(|| ReportEntityDto {
                identity,
                ..Default::default()
            });
        let (fields, partial) = if family == "RegistryValue" {
            system_fields(attrs)
        } else {
            selected(attrs, names)
        };
        section.partial |= partial;
        entity.fields.extend(fields);
        increment(&mut entity.counts, family, 1)?;
        if family == "EmailMessage" {
            increment(&mut entity.counts, "attachments", attachments)?;
        }
        for field in ["visitTime", "startTime", "sentAt", "receivedAt"] {
            if let Ok(time) = chrono::DateTime::parse_from_rfc3339(text(attrs, field)) {
                let time = time.with_timezone(&chrono::Utc).to_rfc3339();
                extend_time(&mut entity.fields, "firstSeen", &time, true);
                extend_time(&mut entity.fields, "lastSeen", &time, false);
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> ReportSourceSummaryDto {
        let [operating_system, hardware, browsers, email] = self.sections.map(Section::finish);
        ReportSourceSummaryDto {
            operating_system,
            hardware,
            browsers,
            email,
        }
    }
}

impl Section {
    fn finish(self) -> ReportSectionDto {
        ReportSectionDto {
            record_count: self.records,
            partial: self.partial,
            entities: self.entities.into_values().collect(),
            counts: self.counts,
        }
    }
}

fn increment(
    counts: &mut BTreeMap<String, u64>,
    name: &str,
    count: u64,
) -> Result<(), AnalysisServiceError> {
    let total = counts.entry(name.to_owned()).or_default();
    *total = total
        .checked_add(count)
        .ok_or_else(|| AnalysisServiceError::Other("report summary count overflowed".into()))?;
    Ok(())
}

fn extend_time(fields: &mut BTreeMap<String, String>, name: &str, value: &str, minimum: bool) {
    let entry = fields
        .entry(name.to_owned())
        .or_insert_with(|| value.to_owned());
    if (minimum && value < entry.as_str()) || (!minimum && value > entry.as_str()) {
        *entry = value.to_owned();
    }
}
