use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub grpc: GrpcConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub openapi: OpenApiConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub subscribe: SubscribeConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenApiConfig {
    #[serde(default = "default_openapi_enabled")]
    pub enabled: bool,
    #[serde(default = "default_openapi_title")]
    pub title: String,
    #[serde(default = "default_openapi_version")]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_host")]
    pub host: String,
}

#[derive(Debug, Deserialize)]
pub struct GrpcConfig {
    #[serde(default = "default_grpc_address")]
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_origins")]
    pub allowed_origins: Vec<String>,
}

fn default_port() -> u16 {
    8080
}
fn default_host() -> String {
    "::".to_string()
}
fn default_grpc_address() -> String {
    "http://localhost:50051".to_string()
}
fn default_origins() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_openapi_enabled() -> bool {
    true
}
fn default_openapi_title() -> String {
    "Crap CMS REST API".to_string()
}
fn default_openapi_version() -> String {
    "1.0.0".to_string()
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            enabled: default_openapi_enabled(),
            title: default_openapi_title(),
            version: default_openapi_version(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            address: default_grpc_address(),
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: default_origins(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cms_url")]
    pub cms_url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cms_url: default_cms_url(),
        }
    }
}

fn default_cms_url() -> String {
    "http://localhost:3000".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscribeConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Interval between WebSocket keepalive pings (default: "30s").
    /// Accepts human-readable durations: "30s", "1m", "1m30s".
    #[serde(
        default = "default_ping_interval",
        deserialize_with = "deserialize_duration"
    )]
    pub ping_interval: Duration,
    /// Time to wait for the initial subscribe message (default: "10s").
    #[serde(
        default = "default_subscribe_timeout",
        deserialize_with = "deserialize_duration"
    )]
    pub timeout: Duration,
    /// Maximum incoming WebSocket message size (default: "8KB").
    /// Accepts human-readable sizes: "8KB", "16KB", "1MB".
    #[serde(
        default = "default_max_message_size",
        deserialize_with = "deserialize_size"
    )]
    pub max_message_size: usize,
}

impl Default for SubscribeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ping_interval: default_ping_interval(),
            timeout: default_subscribe_timeout(),
            max_message_size: default_max_message_size(),
        }
    }
}

fn default_ping_interval() -> Duration {
    Duration::from_secs(30)
}

fn default_subscribe_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_max_message_size() -> usize {
    8 * 1024
}

impl GatewayConfig {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: GatewayConfig = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.subscribe.enabled {
            let ping = self.subscribe.ping_interval.as_secs();
            anyhow::ensure!(
                (1..=300).contains(&ping),
                "subscribe.ping_interval must be between 1s and 5m"
            );
            let timeout = self.subscribe.timeout.as_secs();
            anyhow::ensure!(
                (1..=60).contains(&timeout),
                "subscribe.timeout must be between 1s and 1m"
            );
            anyhow::ensure!(
                self.subscribe.max_message_size > 0,
                "subscribe.max_message_size must be > 0"
            );
        }
        Ok(())
    }
}

/// Parse a human-readable duration string like "30s", "5m", "1m30s", or a bare
/// integer (treated as seconds).
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();

    // Bare integer → seconds
    if let Ok(secs) = s.parse::<u64>() {
        return Ok(Duration::from_secs(secs));
    }

    let mut total_secs = 0u64;
    let mut num_buf = String::new();

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
        } else {
            let n: u64 = num_buf
                .parse()
                .map_err(|_| format!("invalid duration: {s:?}"))?;
            num_buf.clear();
            match ch {
                's' => total_secs += n,
                'm' => total_secs += n * 60,
                'h' => total_secs += n * 3600,
                _ => return Err(format!("unknown duration unit '{ch}' in {s:?}")),
            }
        }
    }

    if !num_buf.is_empty() {
        return Err(format!("trailing digits without unit in {s:?}"));
    }
    if total_secs == 0 {
        return Err(format!("invalid duration: {s:?}"));
    }

    Ok(Duration::from_secs(total_secs))
}

fn deserialize_duration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    use serde::de::Error;

    let val = toml::Value::deserialize(d)?;
    match &val {
        toml::Value::Integer(n) => {
            let secs: u64 = (*n)
                .try_into()
                .map_err(|_| D::Error::custom(format!("duration must be non-negative, got {n}")))?;
            Ok(Duration::from_secs(secs))
        }
        toml::Value::String(s) => parse_duration(s).map_err(D::Error::custom),
        _ => Err(D::Error::custom(format!(
            "expected integer or duration string, got {val}"
        ))),
    }
}

/// Parse a human-readable byte size like "8KB", "1MB", or a bare integer (bytes).
fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim();

    if let Ok(n) = s.parse::<usize>() {
        return Ok(n);
    }

    let s_upper = s.to_uppercase();
    let (num_str, multiplier) = if let Some(n) = s_upper.strip_suffix("MB") {
        (n, 1024 * 1024)
    } else if let Some(n) = s_upper.strip_suffix("KB") {
        (n, 1024)
    } else if let Some(n) = s_upper.strip_suffix('B') {
        (n, 1)
    } else {
        return Err(format!("invalid size: {s:?} (use e.g. \"8KB\", \"1MB\")"));
    };

    let n: usize = num_str
        .trim()
        .parse()
        .map_err(|_| format!("invalid size: {s:?}"))?;

    Ok(n * multiplier)
}

fn deserialize_size<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    use serde::de::Error;

    let val = toml::Value::deserialize(d)?;
    match &val {
        toml::Value::Integer(n) => {
            let size: usize = (*n)
                .try_into()
                .map_err(|_| D::Error::custom(format!("size must be non-negative, got {n}")))?;
            Ok(size)
        }
        toml::Value::String(s) => parse_size(s).map_err(D::Error::custom),
        _ => Err(D::Error::custom(format!(
            "expected integer or size string, got {val}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn parse_duration_combined() {
        assert_eq!(parse_duration("1m30s").unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn parse_duration_bare_integer() {
        assert_eq!(parse_duration("10").unwrap(), Duration::from_secs(10));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("30x").is_err());
        assert!(parse_duration("30").is_ok()); // bare int is valid
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn parse_size_kb() {
        assert_eq!(parse_size("8KB").unwrap(), 8192);
    }

    #[test]
    fn parse_size_mb() {
        assert_eq!(parse_size("1MB").unwrap(), 1_048_576);
    }

    #[test]
    fn parse_size_bytes() {
        assert_eq!(parse_size("4096B").unwrap(), 4096);
    }

    #[test]
    fn parse_size_bare_integer() {
        assert_eq!(parse_size("8192").unwrap(), 8192);
    }

    #[test]
    fn parse_size_case_insensitive() {
        assert_eq!(parse_size("8kb").unwrap(), 8192);
        assert_eq!(parse_size("1mb").unwrap(), 1_048_576);
    }

    #[test]
    fn parse_size_invalid() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("8GB").is_err());
    }

    #[test]
    fn toml_subscribe_human_readable() {
        let toml_str = r#"
            [subscribe]
            enabled = true
            ping_interval = "1m"
            timeout = "15s"
            max_message_size = "16KB"
        "#;
        let cfg: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.subscribe.ping_interval, Duration::from_secs(60));
        assert_eq!(cfg.subscribe.timeout, Duration::from_secs(15));
        assert_eq!(cfg.subscribe.max_message_size, 16384);
    }

    #[test]
    fn toml_subscribe_bare_integers() {
        let toml_str = r#"
            [subscribe]
            enabled = true
            ping_interval = 30
            timeout = 10
            max_message_size = 8192
        "#;
        let cfg: GatewayConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.subscribe.ping_interval, Duration::from_secs(30));
        assert_eq!(cfg.subscribe.timeout, Duration::from_secs(10));
        assert_eq!(cfg.subscribe.max_message_size, 8192);
    }

    #[test]
    fn validate_rejects_zero_ping() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                ping_interval: Duration::ZERO,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_excessive_ping() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                ping_interval: Duration::from_secs(600),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_passes_when_disabled() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: false,
                ping_interval: Duration::ZERO,
                timeout: Duration::ZERO,
                max_message_size: 0,
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_timeout() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                timeout: Duration::ZERO,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_excessive_timeout() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                timeout: Duration::from_secs(120),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_max_message_size() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                max_message_size: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn toml_rejects_negative_duration() {
        let toml_str = r#"
            [subscribe]
            enabled = true
            ping_interval = -5
        "#;
        let result: Result<GatewayConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn toml_rejects_negative_size() {
        let toml_str = r#"
            [subscribe]
            enabled = true
            max_message_size = -1024
        "#;
        let result: Result<GatewayConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn parse_duration_trailing_digits_rejected() {
        assert!(parse_duration("30s10").is_err());
    }

    #[test]
    fn parse_duration_zero_seconds_rejected() {
        // "0s" results in total_secs == 0, rejected
        assert!(parse_duration("0s").is_err());
    }

    #[test]
    fn parse_size_zero_bytes() {
        assert_eq!(parse_size("0B").unwrap(), 0);
        assert_eq!(parse_size("0").unwrap(), 0);
    }

    #[test]
    fn parse_size_whitespace_handling() {
        assert_eq!(parse_size("  8KB  ").unwrap(), 8192);
    }

    #[test]
    fn validate_valid_subscribe_config() {
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                ping_interval: Duration::from_secs(30),
                timeout: Duration::from_secs(10),
                max_message_size: 8192,
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_boundary_values() {
        // Min valid values
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                ping_interval: Duration::from_secs(1),
                timeout: Duration::from_secs(1),
                max_message_size: 1,
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());

        // Max valid values
        let cfg = GatewayConfig {
            subscribe: SubscribeConfig {
                enabled: true,
                ping_interval: Duration::from_secs(300),
                timeout: Duration::from_secs(60),
                max_message_size: usize::MAX,
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }
}
