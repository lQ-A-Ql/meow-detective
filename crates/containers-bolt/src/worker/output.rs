use std::io::{self, Write};
use transport::dto::bolt_reader::BoltReadResponseDto;

struct BoundedWriter<W> {
    writer: W,
    remaining: u64,
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "output limit exceeded",
            ));
        }
        let written = self.writer.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

pub(super) fn write(
    response: &BoltReadResponseDto,
    limit: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut writer = BoundedWriter {
        writer: io::stdout().lock(),
        remaining: limit,
    };
    serde_json::to_writer(&mut writer, response)?;
    writer.flush()?;
    Ok(())
}
