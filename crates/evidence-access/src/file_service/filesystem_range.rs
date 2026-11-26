use std::io::{self, Read, Seek, SeekFrom};

use evidence_core::FileSystemReader;

pub fn open_filesystem_range_file(
    filesystem: Box<dyn FileSystemReader + Send>,
    path: String,
    size: u64,
) -> io::Result<Box<dyn evidence_core::ReadSeek>> {
    filesystem.read_file_range(&path, 0, 0)?;
    Ok(Box::new(FilesystemRangeFile {
        filesystem,
        path,
        size,
        position: 0,
    }))
}

struct FilesystemRangeFile {
    filesystem: Box<dyn FileSystemReader + Send>,
    path: String,
    size: u64,
    position: u64,
}

impl Read for FilesystemRangeFile {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self
            .size
            .saturating_sub(self.position)
            .min(output.len() as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        let bytes = self
            .filesystem
            .read_file_range(&self.path, self.position, count)?;
        if bytes.len() != count {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "filesystem range is shorter than the published file size",
            ));
        }
        output[..count].copy_from_slice(&bytes);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for FilesystemRangeFile {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        let position = match target {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
            SeekFrom::End(delta) => i128::from(self.size) + i128::from(delta),
        };
        self.position = u64::try_from(position)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "file seek exceeds range"))?;
        Ok(self.position)
    }
}
