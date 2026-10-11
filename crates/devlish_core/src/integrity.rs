//! Exact-byte integrity checks. Expected digests must come from a trusted source.
use crate::sha256_hex;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

/// Read once and verify that very buffer; callers must consume the returned
/// bytes rather than reopen the path. This does not authenticate the digest.
pub fn read_verified(path: &Path, expected: &str) -> Result<Vec<u8>, String> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected SHA-256 must contain exactly 64 hexadecimal characters".into());
    }
    let bytes = read_regular_file(path)?;
    let actual = sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "SHA-256 mismatch for {}: expected {expected}, actual {actual}",
            path.display()
        ));
    }
    Ok(bytes)
}

pub fn read_regular_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Inspect the opened handle without waiting for a FIFO writer.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(bytes)
}
