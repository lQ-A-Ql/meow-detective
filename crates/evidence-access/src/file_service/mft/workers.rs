use std::thread::JoinHandle;

use persistence_sqlite::{DbError, DbResult};

use super::reader::MftReaderHandle;

pub(super) fn join_workers(
    reader: MftReaderHandle,
    parsers: Vec<JoinHandle<()>>,
    warnings: &mut Vec<String>,
) -> DbResult<()> {
    let reader_error = match reader.join() {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) => {
            tracing::error!("MFT reader thread panicked: {:?}", error);
            Some("MFT reader worker panicked".to_string())
        }
    };
    let mut parser_panicked = false;
    for parser in parsers {
        if let Err(error) = parser.join() {
            warnings.push(format!("MFT parser thread panicked: {error:?}"));
            tracing::error!("MFT parser thread panicked: {:?}", error);
            parser_panicked = true;
        }
    }
    if let Some(error) = reader_error {
        return Err(DbError::System(format!("MFT reader failure: {error}")));
    }
    if parser_panicked {
        return Err(DbError::System(
            "one or more MFT parser workers panicked".to_string(),
        ));
    }
    Ok(())
}
