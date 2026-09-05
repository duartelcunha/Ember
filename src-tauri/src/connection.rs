//! A connection's destination is part of its credential identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConnection {
    pub base_url: String,
}

impl ProviderConnection {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let raw = if raw.is_empty() {
            ember_core::providers::DEFAULT_OPENAI_BASE_URL
        } else {
            raw
        };
        let url = reqwest::Url::parse(raw).map_err(|_| "Enter a valid HTTPS provider URL")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Provider URL must use HTTPS without embedded credentials, query or fragment"
                    .into(),
            );
        }
        Ok(Self {
            base_url: url.as_str().trim_end_matches('/').to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_validation_rejects_unsafe_or_ambiguous_urls() {
        for bad in [
            "http://example.com/v1",
            "file:///tmp/key",
            "https://user:pass@example.com",
            "https://example.com/?key=x",
            "https://example.com/#fragment",
        ] {
            assert!(ProviderConnection::parse(bad).is_err());
        }
        assert_eq!(
            ProviderConnection::parse(" https://EXAMPLE.com/v1/ ")
                .unwrap()
                .base_url,
            "https://example.com/v1"
        );
    }
}
