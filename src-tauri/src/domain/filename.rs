use super::error::{AppError, ErrorCode};
use std::path::{Component, Path};

pub fn sanitize_filename(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            ErrorCode::PathNotAllowed,
            "Filename cannot be empty",
        ));
    }

    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed);

    let mut cleaned: String = base
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        cleaned = "download".to_string();
    }

    // Windows reserved device names
    let stem = cleaned.split('.').next().unwrap_or(&cleaned);
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "LPT1"
    ) {
        cleaned = format!("_{cleaned}");
    }

    if Path::new(&cleaned)
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::RootDir))
    {
        return Err(AppError::new(
            ErrorCode::PathNotAllowed,
            "Filename must be a basename without path traversal",
        ));
    }

    Ok(cleaned)
}

pub fn filename_from_url(url: &str) -> String {
    UrlPathName::extract(url).unwrap_or_else(|| "download.bin".to_string())
}

struct UrlPathName;

impl UrlPathName {
    fn extract(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let last = parsed.path_segments()?.next_back()?;
        if last.is_empty() {
            return None;
        }
        sanitize_filename(last).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_path_components() {
        assert_eq!(sanitize_filename("../evil/../../passwd").unwrap(), "passwd");
    }

    #[test]
    fn replaces_illegal_chars() {
        assert_eq!(sanitize_filename("a:b?.bin").unwrap(), "a_b_.bin");
    }

    #[test]
    fn handles_reserved_names() {
        assert_eq!(sanitize_filename("CON").unwrap(), "_CON");
    }
}
