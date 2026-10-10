use super::error::{AppError, ErrorCode};
use std::net::IpAddr;
use url::{Host, Url};

/// Production accepts public HTTPS only; loopback is an explicit debug fixture exception.
pub fn validate_download_url(raw: &str) -> Result<Url, AppError> {
    validate(raw, cfg!(debug_assertions))
}
fn validate(raw: &str, fixtures: bool) -> Result<Url, AppError> {
    let url = Url::parse(raw)
        .map_err(|_| AppError::new(ErrorCode::InvalidUrl, "Enter a valid download URL"))?;
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(AppError::new(
            ErrorCode::InvalidUrl,
            "Use a download link without embedded credentials or a fragment.",
        ));
    }
    let local = match url.host() {
        Some(Host::Ipv4(ip)) => {
            if !address_allowed(ip.into(), fixtures) {
                return Err(crate::infrastructure::network::policy_error());
            }
            ip.is_loopback()
        }
        Some(Host::Ipv6(ip)) => {
            if !address_allowed(ip.into(), fixtures) {
                return Err(crate::infrastructure::network::policy_error());
            }
            ip.is_loopback()
        }
        Some(Host::Domain(host)) => host == "localhost",
        None => {
            return Err(AppError::new(
                ErrorCode::InvalidUrl,
                "The download link needs a host.",
            ));
        }
    };
    if local && !fixtures {
        return Err(crate::infrastructure::network::policy_error());
    }
    if url.scheme() != "https" && !(fixtures && local && url.scheme() == "http") {
        return Err(AppError::new(
            ErrorCode::UnsupportedScheme,
            "Use a public HTTPS download link.",
        ));
    }
    Ok(url)
}

pub fn address_allowed(ip: IpAddr, fixtures: bool) -> bool {
    if fixtures && ip.is_loopback() {
        return true;
    }
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(matches!(a, 0 | 10 | 127 | 224..=255)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || b == 0 && matches!(c, 0 | 2) || b == 88 && c == 99))
                || (a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && !matches!(s[0], 0x2002 | 0x3ffe | 0x3fff)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_https_and_debug_loopback_are_explicit() {
        assert!(validate("https://example.com/a", false).is_ok());
        for url in [
            "http://example.com/a",
            "http://127.0.0.1/a",
            "https://localhost/a",
            "https://10.0.0.1/a",
            "https://user:secret@example.com/a",
            "https://example.com/a#token",
            "file:///tmp/a",
            "not a url",
        ] {
            assert!(validate(url, false).is_err(), "{url}");
        }
        assert!(validate("http://127.0.0.1/a", true).is_ok());
        assert!(validate("http://[::1]/a", true).is_ok());
    }
    #[test]
    fn denies_private_special_and_mapped_addresses() {
        for ip in [
            "0.0.0.0",
            "10.1.2.3",
            "100.64.1.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.1.1",
            "192.168.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.2",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
            "3ffe::1",
        ] {
            assert!(!address_allowed(ip.parse().unwrap(), false), "{ip}");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "192.0.10.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(address_allowed(ip.parse().unwrap(), false));
        }
    }
}
