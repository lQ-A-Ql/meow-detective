use std::io;

pub fn path_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("path not found: {path}"))
}

pub fn file_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("file not found: {path}"))
}

pub fn path_is_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is a directory"),
    )
}

pub fn path_is_not_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is not a directory"),
    )
}

pub fn invalid_fs_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub fn unsupported_fs(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

pub fn unexpected_fs_eof(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, message.into())
}

pub fn fs_out_of_memory(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, message.into())
}
