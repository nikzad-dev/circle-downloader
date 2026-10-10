use super::error::{AppError, ErrorCode};
use url::Url;

/// Allow https everywhere; allow http only for loopback (fixture/integration tests).
pub fn validate_download_url(raw: &str) -> Result<Url, AppError> {
    let url = Url::parse(raw).map_err(|_| {
        AppError::new(ErrorCode::InvalidUrl, format!("Invalid URL: {raw}"))
    })?;

    match url.scheme() {
        "https" => Ok(url),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if matches!(host, "127.0.0.1" | "localhost" | "::1") {
                Ok(url)
            } else {
                Err(AppError::new(
                    ErrorCode::UnsupportedScheme,
                    "Only https URLs are allowed (http permitted for localhost)",
                ))
            }
        }
        _ => Err(AppError::new(
            ErrorCode::UnsupportedScheme,
            format!("Unsupported URL scheme: {}", url.scheme()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https() {
        assert!(validate_download_url("https://example.com/a.bin").is_ok());
    }

    #[test]
    fn rejects_plain_http_remote() {
        let err = validate_download_url("http://example.com/a.bin").unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedScheme);
    }

    #[test]
    fn allows_localhost_http() {
        assert!(validate_download_url("http://127.0.0.1:9/file").is_ok());
        assert!(validate_download_url("http://localhost:9/file").is_ok());
    }

    #[test]
    fn rejects_garbage() {
        let err = validate_download_url("not a url").unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidUrl);
    }
}
