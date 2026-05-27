//! 매직 바이트로 외부 압축 포맷 감지.
//!
//! 같은 magic 검사 패턴은 file(1) 명령과 호환되는 보편적 방법.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalFormat {
    /// qsafe 자체 포맷
    Qsafe,
    /// RAR (v4 또는 v5) — 풀기만 가능
    Rar,
    /// ZIP (.zip, .docx, .xlsx, .pptx, .jar, .apk 등 사실 모두 ZIP)
    Zip,
    /// 단일 gzip (.gz)
    Gzip,
    /// 단일 xz / LZMA2 (.xz)
    Xz,
    /// 단일 bzip2 (.bz2)
    Bzip2,
    /// 7-Zip (.7z)
    SevenZ,
    /// LZ4 frame (.lz4)
    Lz4Frame,
    /// zstd 단일 (.zst)
    Zstd,
    /// tar (압축 없음)
    Tar,
    /// 알 수 없는 포맷
    Unknown,
}

impl ExternalFormat {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Qsafe => "qsafe",
            Self::Rar => "rar",
            Self::Zip => "zip",
            Self::Gzip => "gzip",
            Self::Xz => "xz",
            Self::Bzip2 => "bzip2",
            Self::SevenZ => "7z",
            Self::Lz4Frame => "lz4",
            Self::Zstd => "zstd",
            Self::Tar => "tar",
            Self::Unknown => "unknown",
        }
    }

    /// 이 포맷을 qsafe가 풀 수 있는가?
    pub fn can_extract(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// 이 포맷으로 qsafe가 만들 수 있는가? (RAR은 불가)
    pub fn can_create(&self) -> bool {
        !matches!(self, Self::Rar | Self::Unknown)
    }
}

/// 파일 시작 바이트로 포맷 감지.
pub fn detect_format(bytes: &[u8]) -> ExternalFormat {
    if bytes.len() < 4 {
        return ExternalFormat::Unknown;
    }

    // qsafe — 8 byte magic
    if bytes.len() >= 8 && &bytes[..8] == b"QSAFE001" {
        return ExternalFormat::Qsafe;
    }

    // RAR 5.0: 52 61 72 21 1A 07 01 00  ("Rar!\x1A\x07\x01\x00")
    if bytes.len() >= 8 && &bytes[..8] == b"Rar!\x1a\x07\x01\x00" {
        return ExternalFormat::Rar;
    }
    // RAR 4.x: 52 61 72 21 1A 07 00  ("Rar!\x1A\x07\x00")
    if bytes.len() >= 7 && &bytes[..7] == b"Rar!\x1a\x07\x00" {
        return ExternalFormat::Rar;
    }

    // ZIP: PK\x03\x04 (local file header) 또는 PK\x05\x06 (빈 zip 끝)
    if &bytes[..4] == b"PK\x03\x04" || &bytes[..4] == b"PK\x05\x06" {
        return ExternalFormat::Zip;
    }

    // gzip: 1f 8b
    if bytes[0] == 0x1f && bytes[1] == 0x8b {
        return ExternalFormat::Gzip;
    }

    // xz: FD 37 7A 58 5A 00
    if bytes.len() >= 6 && &bytes[..6] == b"\xfd7zXZ\x00" {
        return ExternalFormat::Xz;
    }

    // bzip2: BZh
    if &bytes[..3] == b"BZh" {
        return ExternalFormat::Bzip2;
    }

    // 7z: 37 7A BC AF 27 1C
    if bytes.len() >= 6 && &bytes[..6] == b"7z\xbc\xaf\x27\x1c" {
        return ExternalFormat::SevenZ;
    }

    // LZ4 frame: 04 22 4d 18
    if &bytes[..4] == b"\x04\x22\x4d\x18" {
        return ExternalFormat::Lz4Frame;
    }

    // zstd: 28 B5 2F FD
    if &bytes[..4] == b"\x28\xb5\x2f\xfd" {
        return ExternalFormat::Zstd;
    }

    // tar: 257 bytes 오프셋의 "ustar"  (POSIX) — 헤더 전체가 필요
    if bytes.len() >= 263 && &bytes[257..263] == b"ustar\x00" {
        return ExternalFormat::Tar;
    }
    if bytes.len() >= 263 && &bytes[257..262] == b"ustar" {
        return ExternalFormat::Tar;
    }

    ExternalFormat::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_qsafe() {
        assert_eq!(detect_format(b"QSAFE001..."), ExternalFormat::Qsafe);
    }

    #[test]
    fn detect_rar5() {
        let mut bytes = Vec::from(*b"Rar!\x1a\x07\x01\x00");
        bytes.extend_from_slice(&[0u8; 100]);
        assert_eq!(detect_format(&bytes), ExternalFormat::Rar);
    }

    #[test]
    fn detect_rar4() {
        let mut bytes = Vec::from(*b"Rar!\x1a\x07\x00");
        bytes.extend_from_slice(&[0u8; 100]);
        assert_eq!(detect_format(&bytes), ExternalFormat::Rar);
    }

    #[test]
    fn detect_zip() {
        assert_eq!(detect_format(b"PK\x03\x04...."), ExternalFormat::Zip);
    }

    #[test]
    fn detect_gzip() {
        assert_eq!(detect_format(b"\x1f\x8b\x08\x00..."), ExternalFormat::Gzip);
    }

    #[test]
    fn detect_xz() {
        assert_eq!(detect_format(b"\xfd7zXZ\x00..."), ExternalFormat::Xz);
    }

    #[test]
    fn detect_bzip2() {
        assert_eq!(detect_format(b"BZh91AY&SY"), ExternalFormat::Bzip2);
    }

    #[test]
    fn detect_7z() {
        let mut bytes = Vec::from(*b"7z\xbc\xaf\x27\x1c");
        bytes.extend_from_slice(&[0u8; 10]);
        assert_eq!(detect_format(&bytes), ExternalFormat::SevenZ);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(
            detect_format(b"\xff\xff\xff\xff..."),
            ExternalFormat::Unknown
        );
    }

    #[test]
    fn rar_cannot_create() {
        assert!(!ExternalFormat::Rar.can_create());
        assert!(ExternalFormat::Rar.can_extract());
    }

    #[test]
    fn zip_can_both() {
        assert!(ExternalFormat::Zip.can_create());
        assert!(ExternalFormat::Zip.can_extract());
    }
}
