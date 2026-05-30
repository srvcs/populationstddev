use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub log_level: String,
    pub environment: String,
    /// Base URL of the srvcs-populationvariance dependency.
    pub populationvariance_url: String,
    /// Base URL of the srvcs-sqrt dependency.
    pub sqrt_url: String,
}

impl Config {
    pub fn from_vars(
        bind: Option<String>,
        log: Option<String>,
        env: Option<String>,
        populationvariance_url: Option<String>,
        sqrt_url: Option<String>,
    ) -> Self {
        let bind_addr = bind
            .unwrap_or_else(|| "0.0.0.0:8080".to_string())
            .parse()
            .expect("SRVCS_BIND_ADDR must be host:port");
        Config {
            bind_addr,
            log_level: log.unwrap_or_else(|| "info,tower_http=info".to_string()),
            environment: env.unwrap_or_else(|| "development".to_string()),
            populationvariance_url: populationvariance_url
                .unwrap_or_else(|| "http://127.0.0.1:8090".to_string()),
            sqrt_url: sqrt_url.unwrap_or_else(|| "http://127.0.0.1:8091".to_string()),
        }
    }

    pub fn from_env() -> Self {
        Self::from_vars(
            std::env::var("SRVCS_BIND_ADDR").ok(),
            std::env::var("RUST_LOG").ok(),
            std::env::var("SRVCS_ENV").ok(),
            std::env::var("SRVCS_POPULATIONVARIANCE_URL").ok(),
            std::env::var("SRVCS_SQRT_URL").ok(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::from_vars(None, None, None, None, None);
        assert_eq!(c.bind_addr.port(), 8080);
        assert_eq!(c.environment, "development");
        assert_eq!(c.populationvariance_url, "http://127.0.0.1:8090");
        assert_eq!(c.sqrt_url, "http://127.0.0.1:8091");
    }

    #[test]
    fn parses_explicit_bind_addr() {
        let c = Config::from_vars(Some("127.0.0.1:9000".into()), None, None, None, None);
        assert_eq!(c.bind_addr.port(), 9000);
    }
}
