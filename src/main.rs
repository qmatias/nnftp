#![feature(exclusive_range_pattern)]

use anyhow::Result;
use async_std::{
    net::IpAddr,
    path::PathBuf,
    task,
};
use clap::{crate_authors, crate_version, Clap};

use nnftp::{
    connection::FTPConnection,
    FTPError,
};
use simplelog::*;
use std::str::FromStr;
use url::Url;

#[derive(Clap, Debug)]
#[clap(version = crate_version!(), author = crate_authors!())]
/// FTP Client
struct Opts {
    /// URL in the form ftp://[user[:password]@]host[:port]/url-path
    url: String,

    /// Path to download the file to
    path: PathBuf,

    #[clap(short, parse(from_occurrences))]
    /// Verbosity level
    verbose: u8,
}

async fn run(opts: Opts) -> Result<(), FTPError> {
    let log_level = match opts.verbose {
        0 => LevelFilter::Off,
        1 => LevelFilter::Warn,
        _ => LevelFilter::Debug,
    };

    TermLogger::init(log_level, Config::default(), TerminalMode::Stderr).ok(); // ignore logging failure

    let url = Url::parse(&opts.url).map_err(|_| FTPError::BadHost)?;

    if url.scheme() != "ftp" {
        return Err(FTPError::BadHost);
    }

    let host = url
        .host_str()
        .and_then(|s| IpAddr::from_str(s).ok())
        .ok_or(FTPError::BadHost)?;
    let port = url.port().unwrap_or(21);

    let mut user = url.username();
    if user.is_empty() {
        user = "anonymous"
    }

    let pass = url.password().unwrap_or("");

    let file_path = url.path();

    let mut connection = FTPConnection::login(host, port, user, pass).await?;
    connection.download(file_path, &opts.path).await?;

    Ok(())
}

fn main() -> Result<(), String> {
    let opts: Opts = Opts::parse();

    task::block_on(run(opts)).map_err(|e| e.to_string())?;

    Ok(())
}
