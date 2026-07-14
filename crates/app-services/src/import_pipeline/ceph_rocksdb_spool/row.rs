use transport::CommandError;

use super::{
    ensure_supported_value_type, spool_error, SpoolPoint, SpoolPointRef, SpoolProvenance,
    SpoolRange, SpoolSourceKind,
};

pub(super) fn map_point(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpoolPoint> {
    Ok(SpoolPoint {
        column_family_id: row.get(0)?,
        user_key: row.get(1)?,
        sequence: row.get(2)?,
        value_type: row.get(3)?,
        value: row.get(4)?,
        provenance: SpoolProvenance {
            source_kind: SpoolSourceKind::decode(row.get(5)?).map_err(to_sql_error)?,
            file_number: row.get(6)?,
            level: row.get(7)?,
            physical_offset: row.get(8)?,
            primary_ordinal: row.get(9)?,
            secondary_ordinal: row.get(10)?,
        },
    })
}

pub(super) fn map_point_ref<'row>(
    row: &'row rusqlite::Row<'_>,
) -> Result<SpoolPointRef<'row>, CommandError> {
    let point = SpoolPointRef {
        column_family_id: row.get(0).map_err(CommandError::from_service_error)?,
        user_key: blob_ref(row, 1)?,
        sequence: row.get(2).map_err(CommandError::from_service_error)?,
        value_type: row.get(3).map_err(CommandError::from_service_error)?,
        value: blob_ref(row, 4)?,
        provenance: SpoolProvenance {
            source_kind: SpoolSourceKind::decode(
                row.get(5).map_err(CommandError::from_service_error)?,
            )?,
            file_number: row.get(6).map_err(CommandError::from_service_error)?,
            level: row.get(7).map_err(CommandError::from_service_error)?,
            physical_offset: row.get(8).map_err(CommandError::from_service_error)?,
            primary_ordinal: row.get(9).map_err(CommandError::from_service_error)?,
            secondary_ordinal: row.get(10).map_err(CommandError::from_service_error)?,
        },
    };
    ensure_supported_value_type(point.value_type)?;
    if point.provenance.file_number == 0 {
        return Err(spool_error("stored point provenance has file number zero"));
    }
    Ok(point)
}

pub(super) fn map_range(row: &rusqlite::Row<'_>) -> rusqlite::Result<SpoolRange> {
    Ok(SpoolRange {
        column_family_id: row.get(0)?,
        start_key: row.get(1)?,
        end_key: row.get(2)?,
        sequence: row.get(3)?,
        provenance: SpoolProvenance {
            source_kind: SpoolSourceKind::decode(row.get(4)?).map_err(to_sql_error)?,
            file_number: row.get(5)?,
            level: row.get(6)?,
            physical_offset: row.get(7)?,
            primary_ordinal: row.get(8)?,
            secondary_ordinal: row.get(9)?,
        },
    })
}

pub(super) fn validate_point(point: SpoolPoint) -> Result<SpoolPoint, CommandError> {
    ensure_supported_value_type(point.value_type)?;
    if point.provenance.file_number == 0 {
        return Err(spool_error("stored point provenance has file number zero"));
    }
    Ok(point)
}

pub(super) fn validate_range(range: SpoolRange) -> Result<SpoolRange, CommandError> {
    if range.start_key > range.end_key || range.provenance.file_number == 0 {
        return Err(spool_error("stored range tombstone is invalid"));
    }
    Ok(range)
}

fn blob_ref<'row>(row: &'row rusqlite::Row<'_>, index: usize) -> Result<&'row [u8], CommandError> {
    match row
        .get_ref(index)
        .map_err(CommandError::from_service_error)?
    {
        rusqlite::types::ValueRef::Blob(value) => Ok(value),
        _ => Err(spool_error("stored mutation blob has an invalid SQL type")),
    }
}

fn to_sql_error(error: CommandError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}
