use super::EnvVar;

static GE_SERVER_PORT: EnvVar = EnvVar::new("GE_SERVER_PORT");

const DEFAULT_SERVER_PORT: &str = include_str!("../../../server-port.txt");

/// Resolves the local HTTP server port, allowing isolated test/dev instances.
pub(crate) fn server_port() -> u16 {
    GE_SERVER_PORT
        .string()
        .as_deref()
        .and_then(parse_port)
        .unwrap_or_else(|| parse_port(DEFAULT_SERVER_PORT).expect("server-port.txt must contain a non-zero u16"))
}

pub(crate) fn loopback_http_url(path: &str) -> String {
    format!("http://127.0.0.1:{}{path}", server_port())
}

fn parse_port(value: &str) -> Option<u16> {
    value.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_default_is_valid() {
        assert_eq!(parse_port(DEFAULT_SERVER_PORT), Some(31337));
    }

    #[test]
    fn rejects_invalid_ports() {
        assert_eq!(parse_port("0"), None);
        assert_eq!(parse_port("65536"), None);
        assert_eq!(parse_port("not-a-port"), None);
    }
}
