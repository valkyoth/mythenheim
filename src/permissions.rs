use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Capability(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityParseError;

impl Capability {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Capability {
    type Err = CapabilityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if is_valid_capability(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(CapabilityParseError)
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl fmt::Display for CapabilityParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid capability string")
    }
}

impl std::error::Error for CapabilityParseError {}

fn is_valid_capability(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };

    if !is_valid_segment(first) {
        return false;
    }

    let mut segment_count = 1;
    for part in parts {
        segment_count += 1;
        if !is_valid_segment(part) {
            return false;
        }
    }

    segment_count >= 2
}

fn is_valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_granular_capability() {
        let capability: Capability = "post.delete.own".parse().unwrap();

        assert_eq!(capability.as_str(), "post.delete.own");
    }

    #[test]
    fn rejects_wildcards_and_uppercase() {
        assert!("post.*".parse::<Capability>().is_err());
        assert!("System.Settings.Write".parse::<Capability>().is_err());
    }

    #[test]
    fn rejects_single_segment() {
        assert!("admin".parse::<Capability>().is_err());
    }
}
