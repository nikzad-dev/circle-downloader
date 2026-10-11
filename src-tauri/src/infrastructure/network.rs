use crate::domain::url_policy::{address_allowed, validate_download_url};
use crate::domain::{AppError, ErrorCode};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
#[error("The download address is blocked by policy")]
pub struct AddressBlocked;

struct PublicDns {
    fixtures: bool,
}
impl Resolve for PublicDns {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_owned();
        let fixtures = self.fixtures;
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            let local_fixture = fixtures && name.eq_ignore_ascii_case("localhost");
            if addresses.is_empty()
                || addresses
                    .iter()
                    .any(|a| !address_allowed(a.ip(), local_fixture))
            {
                return Err(Box::new(AddressBlocked) as Box<dyn std::error::Error + Send + Sync>);
            }
            // The transport consumes these checked addresses; there is no second DNS lookup.
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
pub fn client() -> Result<reqwest::Client, AppError> {
    configured_client(
        &crate::domain::advanced::NetworkSettings::default(),
        None,
        None,
        None,
        cfg!(debug_assertions),
    )
}
#[cfg(test)]
fn client_for(fixtures: bool) -> Result<reqwest::Client, AppError> {
    configured_client(
        &crate::domain::advanced::NetworkSettings::default(),
        None,
        None,
        None,
        fixtures,
    )
}
pub fn configured_client(
    settings: &crate::domain::advanced::NetworkSettings,
    proxy_secret: Option<&super::credentials::Secret>,
    origin_secret: Option<&super::credentials::Secret>,
    authorized_origin: Option<&str>,
    fixtures: bool,
) -> Result<reqwest::Client, AppError> {
    settings.validate()?;
    if origin_secret.is_some() && authorized_origin.is_none() {
        return Err(AppError::new(
            ErrorCode::UrlNotAllowed,
            "Origin credentials require an exact origin authorization.",
        ));
    }
    let origin = authorized_origin.map(str::to_owned);
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(
            settings.connect_timeout_secs,
        ))
        .dns_resolver(Arc::new(PublicDns { fixtures }))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Redirect limit exceeded");
            }
            if !redirect_allowed(attempt.url(), attempt.previous())
                || origin
                    .as_ref()
                    .is_some_and(|o| *o != attempt.url().origin().ascii_serialization())
            {
                return attempt.error(AddressBlocked);
            }
            attempt.follow()
        }));
    if !settings.proxy_url.is_empty() {
        let mut proxy =
            reqwest::Proxy::all(&settings.proxy_url).map_err(super::errors::map_reqwest_err)?;
        if let Some(secret) = proxy_secret {
            proxy = proxy.basic_auth(&secret.username, &secret.password);
        }
        builder = builder.proxy(proxy);
    }
    if let Some(secret) = origin_secret {
        use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
        // Build the Basic value through reqwest, which also marks authorization sensitive.
        let request = reqwest::Client::new()
            .get("https://example.invalid")
            .basic_auth(&secret.username, Some(&secret.password))
            .build()
            .map_err(super::errors::map_reqwest_err)?;
        let mut headers = HeaderMap::new();
        let mut auth: HeaderValue =
            request
                .headers()
                .get(AUTHORIZATION)
                .cloned()
                .ok_or_else(|| {
                    AppError::new(ErrorCode::Internal, "Could not authorize this request.")
                })?;
        auth.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth);
        builder = builder.default_headers(headers);
    }
    builder.build().map_err(super::errors::map_reqwest_err)
}
fn redirect_allowed(next: &url::Url, previous: &[url::Url]) -> bool {
    validate_download_url(next.as_str()).is_ok()
        && !(previous.iter().any(|u| u.scheme() == "https") && next.scheme() != "https")
        && !(previous.iter().any(|u| !is_loopback_url(u)) && is_loopback_url(next))
}
fn is_loopback_url(url: &url::Url) -> bool {
    url.host_str().is_some_and(|h| h == "localhost")
        || match url.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        }
}
pub fn policy_error() -> AppError {
    AppError::new(
        ErrorCode::UrlNotAllowed,
        "This link targets a blocked address or redirect. Use a public HTTPS file link.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn transport_dns_blocks_loopback_in_production_policy() {
        let error = client_for(false)
            .unwrap()
            .get("https://localhost:9/file?signature=secret")
            .send()
            .await
            .unwrap_err();
        let error = crate::infrastructure::errors::map_reqwest_err(error);
        assert_eq!(error.code, ErrorCode::UrlNotAllowed);
        assert!(!error.message.contains("secret"));
    }
    #[tokio::test]
    async fn redirect_to_private_address_is_rejected_before_the_next_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(socket.read(&mut buf).await.unwrap() > 0);
            socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://169.254.169.254/secret?token=private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let error = client()
            .unwrap()
            .get(format!("http://{address}/file"))
            .send()
            .await
            .unwrap_err();
        let error = crate::infrastructure::errors::map_reqwest_err(error);
        assert_eq!(error.code, ErrorCode::UrlNotAllowed);
        assert!(!error.retryable);
        assert!(!error.message.contains("private"));
        server.await.unwrap();
    }
    #[test]
    fn redirect_policy_rejects_https_downgrade_and_public_to_loopback() {
        // A policy attempt is exercised through a loopback origin for the real HTTP test;
        // these URL pairs cover the independent transport rules.
        for (from, to) in [
            ("https://example.com/a", "http://example.com/b"),
            ("https://example.com/a", "https://127.0.0.1/b"),
        ] {
            let from = url::Url::parse(from).unwrap();
            let to = url::Url::parse(to).unwrap();
            assert!(!redirect_allowed(&to, &[from]));
        }
    }
}

#[cfg(test)]
mod advanced_tests {
    use super::*;
    use crate::domain::advanced::NetworkSettings;
    use crate::infrastructure::credentials::Secret;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    #[tokio::test]
    async fn proxy_407_is_classified_without_exposing_proxy_credentials() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let n = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]).to_lowercase();
            assert!(request.contains("proxy-authorization: basic "));
            assert!(!request.contains("\r\nauthorization:"));
            socket.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let settings = NetworkSettings {
            proxy_url: format!("http://{address}"),
            ..Default::default()
        };
        let secret = Secret {
            username: "scoped-user".into(),
            password: "private-password".into(),
        };
        let response = configured_client(&settings, Some(&secret), None, None, true)
            .unwrap()
            .get("http://example.com/file?token=private-token")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED
        );
        let error =
            super::super::errors::http_status_error_with_retry_after(response.status(), None);
        assert_eq!(error.code, ErrorCode::HttpUnauthorized);
        assert!(!error.retryable);
        let text = serde_json::to_string(&error).unwrap();
        assert!(!text.contains("private"));
        assert!(!text.contains("scoped-user"));
        fixture.await.unwrap();
    }
    #[tokio::test]
    async fn proxy_connection_failure_is_redacted_and_scoped_headers_never_reach_a_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let settings = NetworkSettings {
            proxy_url: format!("http://{address}"),
            ..Default::default()
        };
        let e = configured_client(&settings, None, None, None, true)
            .unwrap()
            .get("https://example.com/file?secret=private")
            .send()
            .await
            .unwrap_err();
        let e = super::super::errors::map_reqwest_err(e);
        assert_eq!(e.code, ErrorCode::NetworkConnection);
        assert!(!e.message.contains("private"));
        assert!(!e.message.contains(&address.to_string()));
        let source = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source_address = source.local_addr().unwrap();
        let sink = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let sink_address = sink.local_addr().unwrap();
        let fixture = tokio::spawn(async move {
            let (mut socket, _) = source.accept().await.unwrap();
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(
                String::from_utf8_lossy(&buf[..n])
                    .to_lowercase()
                    .contains("\r\nauthorization: basic ")
            );
            socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{sink_address}/file\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        let secret = Secret {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        // Loopback HTTP is used only by the transport fixture. Production grants require HTTPS.
        let origin = format!("http://{source_address}");
        let e = configured_client(
            &NetworkSettings::default(),
            None,
            Some(&secret),
            Some(&origin),
            true,
        )
        .unwrap()
        .get(format!("{origin}/file"))
        .send()
        .await
        .unwrap_err();
        assert_eq!(
            super::super::errors::map_reqwest_err(e).code,
            ErrorCode::UrlNotAllowed
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), sink.accept())
                .await
                .is_err()
        );
        fixture.await.unwrap();
    }
    #[test]
    fn credentials_without_explicit_origin_are_rejected() {
        let secret = Secret {
            username: "user".into(),
            password: "private".into(),
        };
        assert!(
            configured_client(&NetworkSettings::default(), None, Some(&secret), None, true)
                .is_err()
        );
    }
}
