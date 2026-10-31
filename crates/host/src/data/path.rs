use unicode_normalization::is_nfc;

pub(super) const TEMP_PREFIX: &str = ".lucent-atomic-";

pub(super) fn segments(path: &str) -> Result<Vec<&str>, &'static str> {
    if path.len() > 1024 {
        return Err("invalid-path");
    }
    let segments: Vec<_> = path.split('/').collect();
    if segments.len() > 32 || !is_nfc(path) {
        return Err("invalid-path");
    }
    for segment in &segments {
        let stem = segment.split('.').next().unwrap().to_ascii_lowercase();
        let reserved = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
            || ["com", "lpt"].iter().any(|prefix| {
                stem.strip_prefix(prefix).is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                })
            });
        if segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.len() > 255
            || segment.ends_with(['.', ' '])
            || segment.starts_with(TEMP_PREFIX)
            || segment.chars().any(|c| c < ' ' || "<>:\"\\|?*".contains(c))
            || reserved
        {
            return Err("invalid-path");
        }
    }
    Ok(segments)
}
