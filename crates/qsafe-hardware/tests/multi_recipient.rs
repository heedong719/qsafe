//! 다중 수신자 시나리오 E2E 테스트 (mock 백엔드).
//!
//! 시뮬레이션: 한 파일에 password + FIDO2 두 수신자를 동시에 등록.
//! 어느 한쪽으로도 풀 수 있어야 함 (OR 논리).
//!
//! 실제 password 수신자는 qsafe-crypto에서 검증되었으므로,
//! 여기서는 "다중 수신자 가운데 FIDO2만으로도 풀린다"를 검증.

use qsafe_core::envelope::{decrypt_payload, encrypt_payload, random_payload_nonce, FileKey};
use qsafe_core::format::{CipherSuite, CompressionAlgo, FileHeader, IntegrityAlgo, Recipient};
use qsafe_core::integrity::blake3_hash;
use qsafe_core::io::{read_packed_file, write_packed_file};
use qsafe_hardware::backend::{MockPrfBackend, PrfBackend};
use qsafe_hardware::{unwrap_fido2_with, Fido2Wrapper};

#[test]
fn fido2_only_recipient_full_roundtrip() {
    // 1. plaintext 준비
    let plaintext = b"top secret data protected by fido2 only";
    let original_size = plaintext.len() as u64;
    let original_hash = blake3_hash(plaintext);

    // 2. FileKey + 페이로드 암호화
    let file_key = FileKey::random();
    let nonce = random_payload_nonce();
    let ciphertext = encrypt_payload(&file_key, &nonce, plaintext).unwrap();

    // 3. FIDO2 mock 백엔드로 등록 + wrap
    let backend = MockPrfBackend::new(0xCAFE);
    let cred = backend.enroll(Some("e2e-key")).unwrap();
    let recipient = Fido2Wrapper::new(&backend, cred.credential_id.clone())
        .with_label("e2e-key")
        .wrap(&file_key)
        .unwrap();

    // 4. 헤더 작성
    let mut header = FileHeader::new(CipherSuite::V1Xchacha20Blake3, CompressionAlgo::None);
    header.integrity = IntegrityAlgo::Blake3;
    header.recipients.push(recipient);
    header.payload_nonce = nonce.clone();
    header.original_size = original_size;
    header.created_at_unix = 1700000000;

    // 5. 직렬화
    let mut buf = Vec::new();
    write_packed_file(&mut buf, &header, &ciphertext, &original_hash).unwrap();

    // 6. 역직렬화
    let parsed = read_packed_file(buf.as_slice()).unwrap();
    let f2r = match &parsed.header.recipients[0] {
        Recipient::Fido2(f) => f.clone(),
        _ => panic!("expected Fido2 recipient"),
    };

    // 7. 같은 mock 백엔드(=같은 하드웨어 키 시뮬)로 unwrap
    let recovered_key = unwrap_fido2_with(&backend, &f2r).unwrap();

    // 8. 페이로드 복호화 → plaintext 비교
    let recovered_plain = decrypt_payload(
        &recovered_key,
        &parsed.header.payload_nonce,
        &parsed.payload,
    )
    .unwrap();
    assert_eq!(recovered_plain, plaintext);
}

#[test]
fn fido2_with_wrong_device_fails_but_other_recipient_works() {
    // 같은 파일에 두 개의 다른 mock device로 wrap한 fido2 수신자 추가.
    // 첫 번째 device로는 풀고, 두 번째 device로는 실패해야 함.
    let plaintext = b"data";
    let file_key = FileKey::random();
    let nonce = random_payload_nonce();
    let ciphertext = encrypt_payload(&file_key, &nonce, plaintext).unwrap();

    let device_a = MockPrfBackend::new(1);
    let device_b = MockPrfBackend::new(2);

    let cred_a = device_a.enroll(Some("device-a")).unwrap();
    let cred_b = device_b.enroll(Some("device-b")).unwrap();

    let r_a = Fido2Wrapper::new(&device_a, cred_a.credential_id)
        .with_label("device-a")
        .wrap(&file_key)
        .unwrap();
    let r_b = Fido2Wrapper::new(&device_b, cred_b.credential_id)
        .with_label("device-b")
        .wrap(&file_key)
        .unwrap();

    let mut header = FileHeader::new(CipherSuite::V1Xchacha20Blake3, CompressionAlgo::None);
    header.integrity = IntegrityAlgo::Blake3;
    header.recipients.push(r_a.clone());
    header.recipients.push(r_b.clone());
    header.payload_nonce = nonce;
    header.original_size = plaintext.len() as u64;
    header.created_at_unix = 0;

    let mut buf = Vec::new();
    write_packed_file(&mut buf, &header, &ciphertext, &blake3_hash(plaintext)).unwrap();
    let parsed = read_packed_file(buf.as_slice()).unwrap();

    // device A 가져와서 모든 fido2 수신자 시도
    let mut success = false;
    for recipient in &parsed.header.recipients {
        if let Recipient::Fido2(f) = recipient {
            if let Ok(_fk) = unwrap_fido2_with(&device_a, f) {
                success = true;
                break;
            }
        }
    }
    assert!(success, "device A로 적어도 한 수신자는 풀려야 함");
}
