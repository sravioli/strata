//! L0 tests of `strata-extract` with the real `pdftotext`, outside the sandbox (task 2.2 of change
//! worker-extraction). The test files are in `crates/strata-tests/fixtures/`.

use strata_extract::{Exit, PDFTOTEXT, run};

fn extract(name: &str) -> (Exit, Vec<u8>) {
    let path = format!(
        "{}/../strata-tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut out = Vec::new();
    (run(PDFTOTEXT, &bytes[..], &mut out), out)
}

fn document(name: &str) -> serde_json::Value {
    let (code, out) = extract(name);
    assert_eq!(code, Exit::Ok, "{name}");
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn the_known_text_of_a_pdf() {
    let expected = serde_json::json!({"format": "pdf", "pages": 2, "blocks": [
        {"kind": "page", "index": 1, "text": "alpha page one"},
        {"kind": "page", "index": 2, "text": "bravo page two"},
    ]});
    assert_eq!(document("two-pages.pdf"), expected);
}

#[test]
fn a_scanned_pdf_with_no_text_gives_no_blocks() {
    assert_eq!(
        document("scan-no-text.pdf"),
        serde_json::json!({"format": "pdf", "pages": 1, "blocks": []})
    );
}

#[test]
fn a_pdf_with_only_an_owner_password_opens() {
    assert_eq!(document("owner-password.pdf"), document("two-pages.pdf"));
}

#[test]
fn a_pdf_with_a_user_password_is_an_encrypted_file() {
    assert_eq!(
        extract("user-password.pdf"),
        (Exit::EncryptedFile, Vec::new())
    );
}

#[test]
fn a_pdf_with_a_broken_structure_is_a_malformed_file() {
    assert_eq!(extract("broken.pdf"), (Exit::MalformedFile, Vec::new()));
}

#[test]
fn a_zip_archive_with_the_name_report_pdf_is_not_a_pdf() {
    assert_eq!(extract("report.pdf"), (Exit::UnsupportedFormat, Vec::new()));
}
