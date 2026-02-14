use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Generate a hash from a list of paths
/// Paths are sorted alphabetically (case-sensitive) before hashing
/// Returns a 12-character hex string
pub fn hash_paths(paths: &[PathBuf]) -> String {
    let mut sorted_paths: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Sort case-sensitively
    sorted_paths.sort();

    // Create hash input by joining sorted paths
    let combined = sorted_paths.join("\n");

    // Generate SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    let result = hasher.finalize();

    // Convert to hex and truncate to 12 characters
    let hex_string = hex::encode(result);
    hex_string[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_stability() {
        let paths1 = vec![
            PathBuf::from("/home/user/path1"),
            PathBuf::from("/home/user/path2"),
        ];

        let paths2 = vec![
            PathBuf::from("/home/user/path2"),
            PathBuf::from("/home/user/path1"),
        ];

        // Order shouldn't matter - should produce same hash
        assert_eq!(hash_paths(&paths1), hash_paths(&paths2));
    }

    #[test]
    fn test_hash_length() {
        let paths = vec![PathBuf::from("/some/path")];
        let hash = hash_paths(&paths);
        assert_eq!(hash.len(), 12);
    }
}
