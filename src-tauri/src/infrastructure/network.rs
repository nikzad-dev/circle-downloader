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

pub fn client() -> Result<reqwest::Client, AppError> {
    client_for(cfg!(debug_assertions))
}
fn client_for(fixtures: bool) -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(PublicDns { fixtures }))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("Redirect limit exceeded");
            }
            if !redirect_allowed(attempt.url(), attempt.previous()) {
                return attempt.error(AddressBlocked);
            }
            attempt.follow()
        }))
        .build()
        .map_err(super::errors::map_reqwest_err)
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
