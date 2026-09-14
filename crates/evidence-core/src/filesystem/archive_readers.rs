mod gzip;
mod tar;

pub(crate) use gzip::{skip_exact, GzipSingleEntryReader, GzipTarEntryReader};
pub(crate) use tar::BoundedFileReader;
