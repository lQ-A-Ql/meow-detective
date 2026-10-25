mod input;
mod limits;
mod output;

use crate::{read_bolt, BoltReadOptions, BoltReadReport};
use transport::dto::bolt_reader::*;

pub fn exit_code() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Bolt worker failed: {error}");
            if error
                .downcast_ref::<serde_json::Error>()
                .is_some_and(|error| {
                    error.io_error_kind() == Some(std::io::ErrorKind::FileTooLarge)
                })
            {
                12
            } else {
                2
            }
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request = input::request()?;
    input::validate(&request)?;
    limits::apply(&request.limits)?;
    let result = match execute(&request) {
        Ok(report) => BoltReadResultDto::Complete { report },
        Err(_) => BoltReadResultDto::Failed {
            code: "boltReadFailed".into(),
        },
    };
    let response = BoltReadResponseDto {
        protocol_version: BOLT_READER_PROTOCOL_VERSION,
        request_id: request.request_id,
        result,
    };
    output::write(&response, request.limits.max_output_bytes)?;
    Ok(())
}

fn execute(request: &BoltReadRequestDto) -> Result<BoltReadReportDto, Box<dyn std::error::Error>> {
    let _input = input::verify_file(request)?;
    let options = BoltReadOptions {
        prefix: request.prefix.clone(),
        record_keys: request.record_keys.clone(),
        value_keys: request.value_keys.clone(),
        parent_depth: request.parent_depth.map(|depth| depth as usize),
        include_buckets: request.include_buckets,
        max_file_bytes: request.limits.max_file_bytes,
        bucket_parent_depth: request.bucket_parent_depth.map(|depth| depth as usize),
        parent_suffix: request.parent_suffix.clone(),
        max_entries: request.limits.max_entries as usize,
        max_depth: request.limits.max_depth as usize,
        max_output_bytes: usize::try_from(request.limits.max_output_bytes)?,
    };
    let report = read_bolt(std::path::Path::new(&request.input_path), &options)?;
    Ok(project(report))
}

fn project(report: BoltReadReport) -> BoltReadReportDto {
    BoltReadReportDto {
        page_size: report.metadata.page_size,
        txid: report.metadata.txid,
        metadata_checksum_verified: report.metadata.metadata_checksum_verified,
        visited_entries: report.visited_entries,
        visited_buckets: report.visited_buckets,
        missing_bucket: report.missing_bucket.map(|index| index as u32),
        entries: report
            .entries
            .into_iter()
            .map(|entry| BoltReadEntryDto {
                bucket_path: entry.bucket_path,
                key: entry.key,
                value_length: entry.value_length,
                value: entry.value,
                is_bucket: entry.is_bucket,
            })
            .collect(),
    }
}
