use thiserror::Error;

pub mod connection;

#[derive(Error, Debug)]
pub enum FTPError {
    #[error("Unexpected EOF from server")]
    UnexpectedEOF,

    #[error("Bad host")]
    BadHost,

    #[error("Invalid Response from server")]
    InvalidResponse,

    #[error("Bad status code {0} from server: {1}")]
    BadStatus(u16, String),

    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

pub fn parse_line(line: &str) -> Option<(u16, &str)> {
    let idx = line.find(' ')?;
    let code = &line[0..idx];
    code.parse::<u16>()
        .ok()
        .map(|code| (code, &line[idx + 1..]))
}
