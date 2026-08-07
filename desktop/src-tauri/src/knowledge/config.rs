use crate::config::LocalEmbeddingConfig;
use url::Host;

pub(crate) fn validate_local_embedding(config: &LocalEmbeddingConfig) -> Result<(), &'static str> {
    if config.provider != "ollama_loopback" {
        return Err("KB_EMBEDDING_PROVIDER_UNSUPPORTED");
    }
    if !is_loopback_endpoint(&config.endpoint) {
        return Err("KB_EMBEDDING_ENDPOINT_NOT_LOOPBACK");
    }
    if config.model.trim().is_empty() {
        return Err("KB_EMBEDDING_MODEL_UNAVAILABLE");
    }
    Ok(())
}

fn is_loopback_endpoint(raw: &str) -> bool {
    let Ok(endpoint) = url::Url::parse(raw.trim()) else {
        return false;
    };
    if endpoint.scheme() != "http"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.path() != "/"
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.port().is_some_and(|port| port == 0)
    {
        return false;
    }
    match endpoint.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(address)) => address.octets() == [127, 0, 0, 1],
        Some(Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LocalEmbeddingConfig;

    #[test]
    fn only_exact_loopback_http_endpoints_are_accepted() {
        let mut config = LocalEmbeddingConfig {
            provider: "ollama_loopback".into(),
            endpoint: "http://127.0.0.1:11434".into(),
            model: "nomic".into(),
        };
        assert!(validate_local_embedding(&config).is_ok());
        for endpoint in [
            "http://0.0.0.0:11434",
            "http://192.168.1.2",
            "https://localhost",
            "http://example.com",
            "http://localhost:80@evil.example",
            "http://127.0.0.1:80@evil.example",
            "http://user:password@localhost",
            "http://[::1]:80@evil.example",
            "http://localhost:65536",
            "http://localhost:0",
            "http://localhost/api",
            "http://localhost?model=nomic",
            "http://localhost#fragment",
        ] {
            config.endpoint = endpoint.into();
            assert_eq!(
                validate_local_embedding(&config),
                Err("KB_EMBEDDING_ENDPOINT_NOT_LOOPBACK")
            );
        }
    }
}
