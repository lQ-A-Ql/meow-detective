use domain::DataSourceId;
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, AndroidAnalysisRunDto};

use super::device_facts::collect_device_facts;
use super::model::AndroidFact;
use super::persistence::persist_android_analysis;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::SourceReadContext;

pub(crate) fn run_android_source_analysis(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    data_source_id: &DataSourceId,
) -> Result<AndroidAnalysisRunDto, AnalysisServiceError> {
    let mut warnings = Vec::new();
    let mut scanned_file_count = 0u64;
    let mut facts = collect_device_facts(
        source_conn,
        source_reader,
        &mut warnings,
        &mut scanned_file_count,
    )?;
    let packages = super::package_metadata::collect_packages(
        source_conn,
        source_reader,
        &mut warnings,
        &mut scanned_file_count,
    )?;
    sort_device_facts(&mut facts);
    persist_android_analysis(source_conn, data_source_id, &facts, &packages)?;

    Ok(AndroidAnalysisRunDto {
        status: parse_status(facts.is_empty(), packages.is_empty(), &warnings),
        scanned_file_count,
        device_fact_count: facts.len() as u64,
        package_count: packages.len() as u64,
        warnings,
    })
}

fn sort_device_facts(facts: &mut [AndroidFact]) {
    facts.sort_by(|left, right| left.field.cmp(right.field));
}

fn parse_status(
    facts_empty: bool,
    packages_empty: bool,
    warnings: &[String],
) -> AnalysisParseStatusDto {
    if facts_empty && packages_empty {
        AnalysisParseStatusDto::NotFound
    } else if warnings.is_empty() {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::Partial
    }
}
