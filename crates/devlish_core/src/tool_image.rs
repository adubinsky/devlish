//! Restricted static ELF inspection on the same sealed object a future launcher
//! must consume. This is not a launch capability or complete ELF validator.
use crate::tool_snapshot::SealedToolSnapshot;

pub const PROFILE: &str = devlish_audit::tool_catalog::STATIC_PROFILE;

#[derive(Debug)]
pub struct StaticToolImage {
    snapshot: SealedToolSnapshot,
    layout: Layout,
}

impl StaticToolImage {
    /// Restrict the initial profile to native Linux x86-64, ET_EXEC, no ELF
    /// interpreter/dynamic segment, non-executable stack and disjoint W^X pages.
    /// Catalog authority, CPU compatibility and OS containment remain external.
    pub fn from_snapshot(snapshot: SealedToolSnapshot, profile: &str) -> Result<Self, String> {
        if profile != PROFILE {
            return Err("unsupported tool image profile".into());
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let layout = inspect(&snapshot.read_sealed_bytes()?)?;
            Ok(Self { snapshot, layout })
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = snapshot;
            Err("static tool image profile is unavailable on this platform".into())
        }
    }

    pub fn sha256(&self) -> &str {
        self.snapshot.sha256()
    }
    pub fn entry_address(&self) -> u64 {
        self.layout.entry
    }
    pub fn declared_load_bytes(&self) -> u64 {
        self.layout.load_bytes
    }
    pub fn profile(&self) -> &'static str {
        PROFILE
    }
}

#[cfg(target_os = "linux")]
impl std::os::fd::AsFd for StaticToolImage {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(&self.snapshot)
    }
}

#[derive(Debug)]
struct Layout {
    entry: u64,
    load_bytes: u64,
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn inspect(bytes: &[u8]) -> Result<Layout, String> {
    const INVALID: &str = "tool image does not satisfy the restricted static ELF profile";
    let invalid = || INVALID.to_string();
    fn array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], String> {
        let end = offset.checked_add(N).ok_or(INVALID)?;
        bytes
            .get(offset..end)
            .ok_or(INVALID)?
            .try_into()
            .map_err(|_| INVALID.into())
    }
    let short = |offset| array::<2>(bytes, offset).map(u16::from_le_bytes);
    let word = |offset| array::<4>(bytes, offset).map(u32::from_le_bytes);
    let long = |offset| array::<8>(bytes, offset).map(u64::from_le_bytes);
    if bytes.len() < 64
        || bytes.len() as u64 > devlish_audit::MAX_ARTIFACT_BYTES
        || &bytes[..7] != b"\x7fELF\x02\x01\x01"
        || ![0, 3].contains(&bytes[7])
        || bytes[8..16].iter().any(|b| *b != 0)
        || short(16)? != 2
        || short(18)? != 62
        || word(20)? != 1
        || word(48)? != 0
        || short(52)? != 64
        || short(54)? != 56
    {
        return Err(invalid());
    }
    let entry = long(24)?;
    let offset = usize::try_from(long(32)?).map_err(|_| invalid())?;
    let count = usize::from(short(56)?);
    if offset < 64 || count == 0 || count > 128 {
        return Err(invalid());
    }
    let end = offset.checked_add(count * 56).ok_or_else(invalid)?;
    let table = bytes.get(offset..end).ok_or_else(invalid)?;
    let mut stack = false;
    let mut entry_backed = false;
    let mut load_bytes = 0u64;
    let mut last_page_end = 0u64;
    for header in table.chunks_exact(56) {
        let kind = u32::from_le_bytes(array(header, 0)?);
        let flags = u32::from_le_bytes(array(header, 4)?);
        // PT_INTERP/PT_DYNAMIC must never be delegated to the host loader.
        if kind == 2 || kind == 3 {
            return Err(invalid());
        }
        // Only known non-loader headers are accepted alongside PT_LOAD.
        if ![
            0, 1, 4, 6, 7, 0x6474e550, 0x6474e551, 0x6474e552, 0x6474e553,
        ]
        .contains(&kind)
            || flags & !7 != 0
        {
            return Err(invalid());
        }
        if kind == 0 {
            continue;
        }
        let file_offset = u64::from_le_bytes(array(header, 8)?);
        let address = u64::from_le_bytes(array(header, 16)?);
        let file_size = u64::from_le_bytes(array(header, 32)?);
        let memory_size = u64::from_le_bytes(array(header, 40)?);
        let align = u64::from_le_bytes(array(header, 48)?);
        let file_end = file_offset.checked_add(file_size).ok_or_else(invalid)?;
        let memory_end = address.checked_add(memory_size).ok_or_else(invalid)?;
        if file_end > bytes.len() as u64
            || memory_size < file_size
            || (align > 1 && (!align.is_power_of_two() || file_offset % align != address % align))
        {
            return Err(invalid());
        }
        if kind == 0x6474e551 {
            if stack || flags != 6 || file_size != 0 || memory_size != 0 {
                return Err(invalid());
            }
            stack = true;
        }
        if kind != 1 {
            continue;
        }
        if ![4, 5, 6].contains(&flags)
            || memory_size == 0
            || address < 4096
            || memory_end > (1u64 << 47)
            || file_offset % 4096 != address % 4096
        {
            return Err(invalid());
        }
        let page_start = address & !4095;
        let page_end = memory_end.checked_add(4095).ok_or_else(invalid)? & !4095;
        // Ascending, non-overlapping 4 KiB load pages prevent a later writable
        // segment from remapping an executable segment's page.
        if page_start < last_page_end {
            return Err(invalid());
        }
        last_page_end = page_end;
        load_bytes = load_bytes.checked_add(memory_size).ok_or_else(invalid)?;
        if load_bytes > 256 * 1024 * 1024 {
            return Err(invalid());
        }
        let backed_end = address.checked_add(file_size).ok_or_else(invalid)?;
        if flags & 1 != 0 && entry >= address && entry < backed_end {
            entry_backed = true;
        }
    }
    if !stack || !entry_backed {
        return Err(invalid());
    }
    Ok(Layout { entry, load_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put16(b: &mut [u8], off: usize, value: u16) {
        b[off..off + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn put32(b: &mut [u8], off: usize, value: u32) {
        b[off..off + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn put64(b: &mut [u8], off: usize, value: u64) {
        b[off..off + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        // Synthetic layout only, not executable instructions or a runnable tool.
        let mut bytes = vec![0; 256];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        put16(&mut bytes, 16, 2);
        put16(&mut bytes, 18, 62);
        put32(&mut bytes, 20, 1);
        put64(&mut bytes, 24, 0x4000b0);
        put64(&mut bytes, 32, 64);
        put16(&mut bytes, 52, 64);
        put16(&mut bytes, 54, 56);
        put16(&mut bytes, 56, 2);
        put32(&mut bytes, 64, 1);
        put32(&mut bytes, 68, 5);
        put64(&mut bytes, 80, 0x400000);
        put64(&mut bytes, 96, 256);
        put64(&mut bytes, 104, 256);
        put64(&mut bytes, 112, 4096);
        put32(&mut bytes, 120, 0x6474e551);
        put32(&mut bytes, 124, 6);
        bytes
    }
    #[test]
    fn minimal_static_layout_has_a_file_backed_entry() {
        let bytes = fixture();
        let report = inspect(&bytes).unwrap();
        assert_eq!(report.entry, 0x4000b0);
        assert_eq!(report.load_bytes, 256);
    }
    #[test]
    fn scripts_wrong_architecture_pie_and_dynamic_loaders_are_rejected() {
        assert!(inspect(b"#!/bin/sh\nexit 0\n").is_err());
        for (offset, value) in [(16, 3), (18, 183), (54, 55)] {
            let mut bytes = fixture();
            put16(&mut bytes, offset, value);
            assert!(inspect(&bytes).is_err());
        }
        for kind in [2, 3, 0x70000001] {
            let mut bytes = fixture();
            put32(&mut bytes, 120, kind);
            assert!(inspect(&bytes).is_err());
        }
        for (offset, value) in [(4, 1), (5, 2), (6, 0), (7, 9), (8, 1)] {
            let mut bytes = fixture();
            bytes[offset] = value;
            assert!(inspect(&bytes).is_err());
        }
    }
    #[test]
    fn executable_stack_writable_code_and_unbacked_entry_are_rejected() {
        for flags in [0, 1, 3, 7] {
            let mut bytes = fixture();
            put32(&mut bytes, 68, flags);
            assert!(inspect(&bytes).is_err());
        }
        for flags in [0, 4, 7] {
            let mut bytes = fixture();
            put32(&mut bytes, 124, flags);
            assert!(inspect(&bytes).is_err());
        }
        let mut bytes = fixture();
        put16(&mut bytes, 56, 1);
        assert!(inspect(&bytes).is_err());
        for entry in [0, 0x400100, 0x500000, u64::MAX] {
            let mut bytes = fixture();
            put64(&mut bytes, 24, entry);
            assert!(inspect(&bytes).is_err());
        }
    }
    #[test]
    fn truncated_overflowing_and_excessive_headers_fail_without_panics() {
        let bytes = fixture();
        for end in 0..bytes.len() {
            assert!(inspect(&bytes[..end]).is_err(), "{end}");
        }
        for (offset, value) in [
            (32, u64::MAX),
            (72, u64::MAX),
            (80, u64::MAX),
            (96, u64::MAX),
            (104, u64::MAX),
            (112, 3),
        ] {
            let mut bytes = fixture();
            put64(&mut bytes, offset, value);
            assert!(inspect(&bytes).is_err());
        }
        for count in [0, 129, u16::MAX] {
            let mut bytes = fixture();
            put16(&mut bytes, 56, count);
            assert!(inspect(&bytes).is_err());
        }
    }
    #[test]
    fn overlapping_load_pages_and_oversized_memory_are_rejected() {
        let mut bytes = fixture();
        bytes.resize(1024, 0);
        let load = bytes[64..120].to_vec();
        bytes[176..232].copy_from_slice(&load);
        put16(&mut bytes, 56, 3);
        put32(&mut bytes, 180, 6);
        put64(&mut bytes, 192, 0x400200);
        put64(&mut bytes, 184, 512);
        put64(&mut bytes, 208, 1);
        put64(&mut bytes, 216, 1);
        put64(&mut bytes, 224, 1);
        assert!(inspect(&bytes).is_err());
        put64(&mut bytes, 192, 0x401200);
        assert!(inspect(&bytes).is_ok());
        put64(&mut bytes, 216, 256 * 1024 * 1024);
        assert!(inspect(&bytes).is_err());
    }

    #[test]
    fn mutated_header_bytes_do_not_panic() {
        for offset in 0..176 {
            for value in [0, 1, 127, 255] {
                let mut bytes = fixture();
                bytes[offset] = value;
                assert!(
                    std::panic::catch_unwind(|| inspect(&bytes)).is_ok(),
                    "{offset}:{value}"
                );
            }
        }
    }

    #[test]
    fn loader_page_alignment_and_duplicate_stack_headers_are_rejected() {
        let mut bytes = fixture();
        put64(&mut bytes, 112, 1);
        put64(&mut bytes, 80, 0x400001);
        assert!(inspect(&bytes).is_err());
        let mut bytes = fixture();
        let stack = bytes[120..176].to_vec();
        bytes[176..232].copy_from_slice(&stack);
        put16(&mut bytes, 56, 3);
        assert!(inspect(&bytes).is_err());
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn system_linker_static_fixture_is_accepted_without_executing_it() {
        use std::{fs, process::Command};
        let directory =
            std::env::temp_dir().join(format!("devlish-linked-image-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let source = directory.join("fixture.s");
        let binary = directory.join("fixture");
        fs::write(
            &source,
            r#".text
.global _start
_start:
    mov $60, %rax
    xor %rdi, %rdi
    syscall
.section .note.GNU-stack,"",@progbits
"#,
        )
        .unwrap();
        let linked = Command::new("cc")
            .args([
                "-nostdlib",
                "-static",
                "-no-pie",
                "-Wl,-z,noexecstack",
                "-Wl,--build-id=none",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        let digest = crate::sha256_hex(&fs::read(&binary).unwrap());
        let snapshot = SealedToolSnapshot::from_path(&binary, &digest).unwrap();
        let image = StaticToolImage::from_snapshot(snapshot, PROFILE).unwrap();
        assert_eq!(image.sha256(), digest);
        assert!(image.entry_address() >= 4096);
        // We linked and inspected a synthetic fixture; no child tool was run.
        fs::remove_dir_all(directory).unwrap();
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn typed_image_retains_the_checked_descriptor_after_source_replacement() {
        use std::{
            fs,
            os::fd::{AsFd, AsRawFd},
        };
        let directory =
            std::env::temp_dir().join(format!("devlish-static-image-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("tool");
        let bytes = fixture();
        fs::write(&path, &bytes).unwrap();
        let digest = crate::sha256_hex(&bytes);
        let snapshot = SealedToolSnapshot::from_path(&path, &digest).unwrap();
        let original_fd = snapshot.as_fd().as_raw_fd();
        fs::write(&path, b"#!/bin/sh\nchanged").unwrap();
        let image = StaticToolImage::from_snapshot(snapshot, PROFILE).unwrap();
        assert_eq!(image.as_fd().as_raw_fd(), original_fd);
        assert_eq!(image.sha256(), digest);
        let changed =
            SealedToolSnapshot::from_path(&path, &crate::sha256_hex(b"#!/bin/sh\nchanged"))
                .unwrap();
        assert!(StaticToolImage::from_snapshot(changed, PROFILE).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
