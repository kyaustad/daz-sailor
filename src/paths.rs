use std::path::{Path, PathBuf};

use crate::config::{canonical_library_folder, normalize_library_relative_path};

/// Paths that are safe to ignore during manual library installs.
pub fn should_skip_install_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();

    if lower.starts_with("__macosx/") || lower.contains("/__macosx/") {
        return true;
    }

    if normalized
        .split('/')
        .any(|part| part.starts_with("._") || part.eq_ignore_ascii_case(".ds_store"))
    {
        return true;
    }

    false
}

/// Strip a wrapper prefix from an archive entry, using case-insensitive matching.
pub fn strip_archive_prefix_fuzzy(path: &str, prefix: &str) -> Option<PathBuf> {
    let normalized = path.trim_start_matches("./").replace('\\', "/");

    if prefix.is_empty() {
        return Some(normalize_library_relative_path(Path::new(&normalized)));
    }

    let prefix = prefix.trim_end_matches('/');
    if normalized.len() < prefix.len() {
        return None;
    }

    if !normalized[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }

    let rest = normalized[prefix.len()..].strip_prefix('/').unwrap_or("");
    if rest.is_empty() {
        return None;
    }

    Some(normalize_library_relative_path(Path::new(rest)))
}

/// Returns true when the path should be installed into the DAZ library.
pub fn is_installable_library_path(path: &str, strip_prefix: &str) -> bool {
    if should_skip_install_path(path) {
        return false;
    }

    let Some(relative) = strip_archive_prefix_fuzzy(path, strip_prefix) else {
        return false;
    };

    let first = relative
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();

    canonical_library_folder(&first).is_some() || first.eq_ignore_ascii_case("content")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_macosx_metadata() {
        assert!(should_skip_install_path(
            "Play With Me Pose Pack/__MACOSX/My Library/People/test.duf"
        ));
    }

    #[test]
    fn skips_docs_outside_library_prefix() {
        assert!(!is_installable_library_path(
            "stimuli docs/Stimuli feedback.pdf",
            "Studio/My Library/Presets"
        ));
    }

    #[test]
    fn fuzzy_prefix_is_case_insensitive() {
        let stripped = strip_archive_prefix_fuzzy("My Library/People/test.duf", "my library")
            .unwrap();
        assert_eq!(stripped, Path::new("People/test.duf"));
    }
}
