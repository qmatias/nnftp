use crate::{parse_line, FTPError};
use anyhow::Result;
use async_std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Lines, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    path::Path,
    prelude::*,
    task,
};
use log::{debug, info};
use std::fmt;

const BUF_SIZE: usize = 8000;

#[derive(Debug)]
pub struct Credentials {
    user: String,
    pass: String,
}

impl Default for Credentials {
    fn default() -> Self {
        Credentials {
            user: "anonymous".to_owned(),
            pass: "anonymous".to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct FTPConnection {
    socket: TcpStream,
    lines: Lines<BufReader<TcpStream>>,
}

impl FTPConnection {
    async fn expect<C: AsRef<[u8]> + fmt::Display>(
        &mut self,
        command: C,
        expected: u16,
    ) -> Result<String, FTPError> {
        let (status, msg) = self.command(command).await?;
        if status != expected {
            Err(FTPError::BadStatus(status, msg))
        } else {
            Ok(msg)
        }
    }

    async fn command<C: AsRef<[u8]> + fmt::Display>(
        &mut self,
        command: C,
    ) -> Result<(u16, String), FTPError> {
        debug!("> {}", &command);

        let buf = [command.as_ref(), "\r\n".as_bytes()].concat();

        self.socket.write(&buf).await?;

        let res = self.getline().await?;

        debug!("< {} - {}", res.0, res.1);

        Ok(res)
    }

    async fn getline(&mut self) -> Result<(u16, String), FTPError> {
        // Read server message
        // Format: <status code> <message>
        // Example: 220 FTP server 1.0.0 ready.
        let line = match self.lines.next().await {
            Some(Ok(line)) => line,
            Some(Err(e)) => return Err(e.into()),
            None => return Err(FTPError::UnexpectedEOF),
        };

        match parse_line(&line) {
            Some((code, msg)) => Ok((code, msg.to_owned())),
            None => Err(FTPError::InvalidResponse),
        }
    }

    pub async fn download(&mut self, from: &str, to: impl AsRef<Path>) -> Result<(), FTPError> {
        self.expect("TYPE I", 200).await?;

        let file_size = self
            .expect(["SIZE ", from].concat(), 213)
            .await?
            .parse::<u64>()
            .map_err(|_| FTPError::InvalidResponse)?;

        info!("{} - {} bytes", from, file_size);

        // msg = Entering Passive Mode (145,24,145,107,207,235).
        // IP = 145.24.145.107
        // Port = 53227 (207 * 256 + 235)
        let msg = self.expect("PASV", 227).await?;
        let lparen = msg.find('(').ok_or(FTPError::InvalidResponse)?;
        let rparen = msg.rfind(')').ok_or(FTPError::InvalidResponse)?;
        let addr = &msg[lparen + 1..rparen]
            .split(',')
            .map(|n| n.parse::<u8>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| FTPError::InvalidResponse)?;

        let host = Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3]);
        let port = (addr[4] as u16) * 256 + (addr[5] as u16);

        info!("Passive mode listen address is {}:{}", host, port);

        let mut pasv_conn = TcpStream::connect(SocketAddr::new(host.into(), port)).await?;
        let mut output = BufWriter::new(File::create(to).await?);

        self.expect(["RETR ", from].concat(), 125).await?;

        let mut buf = [0u8; BUF_SIZE];

        loop {
            let count = pasv_conn.read(&mut buf).await?;
            if count == 0 {
                break
            }
            debug!("Writing {} bytes...", count);
            output.write_all(&buf[..count]).await?;
        }

        output.flush().await?;

        Ok(())
    }

    pub async fn login(
        host: impl Into<IpAddr>,
        port: u16,
        user: &str,
        pass: &str,
    ) -> Result<Self, FTPError> {
        let socket = TcpStream::connect(SocketAddr::new(host.into(), port)).await?;
        let mut conn = FTPConnection {
            lines: BufReader::new(socket.clone()).lines(),
            socket,
        };

        let (status, msg) = conn.getline().await?;
        if status != 220 {
            return Err(FTPError::BadStatus(status, msg));
        }

        conn.expect(["USER ", user].concat(), 331).await?;
        conn.expect(["PASS ", pass].concat(), 230).await?;

        info!("Logged in as {}", user);

        Ok(conn)
    }
}
