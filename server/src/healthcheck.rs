//! `legejo healthcheck`: asks the running server for /api/health and exits
//! 0 or 1. It is the container's HEALTHCHECK, so the image needs no curl.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(3);

/// Where to reach the server from inside its own container: the port it
/// listens on, on the loopback address when it listens on all interfaces.
pub fn target(listen: &str) -> Option<SocketAddr> {
    let addr: SocketAddr = listen.parse().ok()?;
    Some(if addr.ip().is_unspecified() {
        let loopback: std::net::IpAddr = if addr.is_ipv4() { [127, 0, 0, 1].into() } else { std::net::Ipv6Addr::LOCALHOST.into() };
        SocketAddr::new(loopback, addr.port())
    } else {
        addr
    })
}

pub fn healthy(addr: SocketAddr) -> bool {
    let request = || -> std::io::Result<String> {
        let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        // One write: a request split over several segments can be answered
        // and closed before the rest arrives, which resets the connection.
        stream.write_all(format!("GET /api/health HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\n\r\n").as_bytes())?;
        let mut response = String::new();
        stream.take(4096).read_to_string(&mut response)?;
        Ok(response)
    };
    request().is_ok_and(|response| response.starts_with("HTTP/1.") && response.split_whitespace().nth(1) == Some("200"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(status_line: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut seen = Vec::new();
                let mut buf = [0u8; 512];
                while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(n) if n > 0 => seen.extend_from_slice(&buf[..n]),
                        _ => break,
                    }
                }
                let _ = stream.write_all(format!("{status_line}\r\ncontent-length: 2\r\n\r\nok").as_bytes());
            }
        });
        addr
    }

    #[test]
    fn reports_the_servers_health() {
        assert!(healthy(serve_once("HTTP/1.1 200 OK")));
        assert!(!healthy(serve_once("HTTP/1.1 503 Service Unavailable")));
        // Nothing listens here.
        let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert!(!healthy(free));
    }

    #[test]
    fn checks_loopback_when_listening_on_all_interfaces() {
        assert_eq!(target("0.0.0.0:3000"), Some("127.0.0.1:3000".parse().unwrap()));
        assert_eq!(target("[::]:8080"), Some("[::1]:8080".parse().unwrap()));
        assert_eq!(target("192.0.2.7:3000"), Some("192.0.2.7:3000".parse().unwrap()));
        assert_eq!(target("not an address"), None);
    }
}
