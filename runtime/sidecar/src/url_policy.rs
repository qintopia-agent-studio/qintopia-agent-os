use anyhow::{bail, Result};

pub fn reject_path_separator_ambiguity(value: &str, label: &str) -> Result<()> {
    let lowered = value.to_ascii_lowercase();
    if value.contains('\\') || lowered.contains("%2f") || lowered.contains("%5c") {
        bail!("{label} must not contain ambiguous path separators");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_raw_or_encoded_path_separators_before_url_parsing() {
        assert!(
            reject_path_separator_ambiguity("https://media.example.test/image.jpg", "media")
                .is_ok()
        );

        for value in [
            "https://media.example.test/poster\\private.jpg",
            "https://media.example.test/poster%5Cprivate.jpg",
            "https://media.example.test/posters%2Fprivate.jpg",
            "https://media.example.test/posters%2fprivate.jpg",
        ] {
            assert!(
                reject_path_separator_ambiguity(value, "media").is_err(),
                "accepted ambiguous URL input {value}"
            );
        }
    }
}
