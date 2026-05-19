use serde::{Deserialize, Serialize};
use std::{fmt, fs, net::SocketAddr, path::Path};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub public_base_url: String,
    pub trusted_proxy_cidrs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DatabaseConfig {
    pub endpoint: String,
    pub namespace: String,
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SecurityConfig {
    pub max_request_body_bytes: u64,
    pub secure_cookies: bool,
    pub csp_report_only: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    Invalid(String),
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = fs::read_to_string(path).map_err(ConfigError::Io)?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(raw).map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server
            .listen_addr
            .parse::<SocketAddr>()
            .map_err(|err| ConfigError::Invalid(format!("server.listen_addr is invalid: {err}")))?;

        validate_http_url("server.public_base_url", &self.server.public_base_url)?;
        validate_database_endpoint(&self.database.endpoint)?;
        validate_identifier("database.namespace", &self.database.namespace)?;
        validate_identifier("database.database", &self.database.database)?;

        if self.security.max_request_body_bytes == 0 {
            return Err(ConfigError::Invalid(
                "security.max_request_body_bytes must be greater than zero".to_owned(),
            ));
        }

        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:37171".to_owned(),
            public_base_url: "http://127.0.0.1:37171".to_owned(),
            trusted_proxy_cidrs: Vec::new(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            endpoint: "ws://127.0.0.1:8000".to_owned(),
            namespace: "mythenheim".to_owned(),
            database: "mythenheim".to_owned(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_request_body_bytes: 1_048_576,
            secure_cookies: true,
            csp_report_only: false,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(formatter, "failed to read config: {err}"),
            Self::Parse(err) => write!(formatter, "failed to parse config: {err}"),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {}

fn validate_http_url(field: &str, value: &str) -> Result<(), ConfigError> {
    if value.starts_with("http://") || value.starts_with("https://") {
        Ok(())
    } else {
        Err(ConfigError::Invalid(format!(
            "{field} must start with http:// or https://"
        )))
    }
}

fn validate_database_endpoint(value: &str) -> Result<(), ConfigError> {
    if value.starts_with("ws://")
        || value.starts_with("wss://")
        || value.starts_with("http://")
        || value.starts_with("https://")
    {
        Ok(())
    } else {
        Err(ConfigError::Invalid(
            "database.endpoint must start with ws://, wss://, http://, or https://".to_owned(),
        ))
    }
}

fn validate_identifier(field: &str, value: &str) -> Result<(), ConfigError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(ConfigError::Invalid(format!(
            "{field} must contain only ASCII letters, numbers, or underscores"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        AppConfig::default().validate().unwrap();
    }

    #[test]
    fn parses_valid_config() {
        let config = AppConfig::parse(
            r#"
            [server]
            listen_addr = "127.0.0.1:4000"
            public_base_url = "https://forum.example.test"

            [database]
            endpoint = "ws://127.0.0.1:9000"
            namespace = "mythenheim_test"
            database = "forum_1"

            [security]
            max_request_body_bytes = 2097152
            secure_cookies = true
            csp_report_only = false
            "#,
        )
        .unwrap();

        assert_eq!(config.database.namespace, "mythenheim_test");
    }

    #[test]
    fn rejects_invalid_database_identifier() {
        let err = AppConfig::parse(
            r#"
            [database]
            namespace = "bad-name"
            "#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("database.namespace"));
    }

    #[test]
    fn rejects_zero_body_limit() {
        let err = AppConfig::parse(
            r#"
            [security]
            max_request_body_bytes = 0
            "#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("max_request_body_bytes"));
    }
}
