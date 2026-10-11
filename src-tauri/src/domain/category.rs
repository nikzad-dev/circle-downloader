use std::collections::HashMap;
use std::sync::OnceLock;

pub const CATEGORIES: &[&str] = &[
    "Compressed",
    "Documents",
    "Music",
    "Pictures",
    "Programs",
    "Videos",
];

/// One extension catalog shared with the UI. Category inference never changes files.
pub fn from_filename(filename: &str) -> String {
    static EXTENSIONS: OnceLock<HashMap<String, String>> = OnceLock::new();
    let extensions = EXTENSIONS.get_or_init(|| {
        let catalog: HashMap<String, Vec<String>> = serde_json::from_str(include_str!(
            "../../../src/features/advanced/category-extensions.json"
        ))
        .expect("valid bundled category catalog");
        catalog
            .into_iter()
            .flat_map(|(category, extensions)| {
                extensions
                    .into_iter()
                    .map(move |extension| (extension, category.clone()))
            })
            .collect()
    });
    let filename = percent_encoding::percent_decode_str(filename).decode_utf8_lossy();
    filename
        .rsplit_once('.')
        .and_then(|(_, extension)| extensions.get(&extension.to_ascii_lowercase()))
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_all_categories_without_guessing_unknown_extensions() {
        for (name, category) in [
            ("archive.tar.gz", "Compressed"),
            ("report.PDF", "Documents"),
            ("report%2Epdf", "Documents"),
            ("song.flac", "Music"),
            ("photo.jpeg", "Pictures"),
            ("installer.AppImage", "Programs"),
            ("movie.mkv", "Videos"),
            ("download", ""),
            ("file.unknown", ""),
            ("video.mp4.exe", "Programs"),
        ] {
            assert_eq!(from_filename(name), category);
        }
    }
}
