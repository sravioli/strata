//! The extraction process of tech design 0007: the file on standard input, one JSON document of
//! tech design 0049 on standard output, or a class as a fixed exit code.
//!
//! The process runs in nsjail as PID 1, with `pids.max` 2. Thus it uses one thread: it writes the
//! bytes to `pdftotext`, closes the pipe, and then reads the output (`pdftotext -` reads all of its
//! input before it writes). The worker (crate `strata`) maps the exit codes to the classes of 0007.

use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Stdio};

/// The maximum size of a file (ADR 0005). The worker sends at most this many bytes.
pub const MAX_INPUT: u64 = 100 * 1024 * 1024;
/// The output limit of 0007. The worker also stops the process at this limit.
pub const MAX_OUTPUT: u64 = 50 * 1024 * 1024;
/// `pdftotext` in the read-only file system of the sandbox.
pub const PDFTOTEXT: &str = "/usr/bin/pdftotext";

/// The exit codes of `strata-extract`. Each is below 128 and is not 255, so it is different from
/// a stop on a signal (128 + the signal) and from a failure of nsjail (255).
pub mod exit {
    /// The JSON document is on standard output.
    pub const OK: i32 = 0;
    pub const UNSUPPORTED_FORMAT: i32 = 10;
    pub const ENCRYPTED_FILE: i32 = 11;
    pub const MALFORMED_FILE: i32 = 12;
    pub const OUTPUT_TOO_LARGE: i32 = 13;
    /// The parser stopped on a signal or did not start, or the input was above the limit.
    pub const CRASH: i32 = 20;
}

/// The format, from the first bytes only. The file name and the content type are not trusted.
#[derive(Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Unsupported,
}

pub fn detect(bytes: &[u8]) -> Format {
    if bytes.starts_with(b"%PDF-") {
        Format::Pdf
    } else {
        Format::Unsupported
    }
}

/// The JSON document of 0049 for the text of `pdftotext`. `pdftotext` ends each page with a form
/// feed. A page with no text gets no block, so a PDF with no text gives an empty list. The text
/// loses each NUL character, because PostgreSQL cannot store it in a `text` value.
pub fn pdf_document(text: &str) -> serde_json::Value {
    let pages = text.matches('\u{c}').count();
    let blocks: Vec<_> = text
        .split('\u{c}')
        .take(pages)
        .enumerate()
        .map(|(i, page)| (i, page.replace('\0', "")))
        .filter(|(_, page)| !page.trim().is_empty())
        .map(|(i, page)| serde_json::json!({"kind": "page", "index": i + 1, "text": page.trim()}))
        .collect();
    serde_json::json!({"format": "pdf", "pages": pages, "blocks": blocks})
}

/// The exit code for a `pdftotext` that did not end with 0. Its exit code 1 means "cannot open the
/// file" for a user password and for a broken file alike, so the bytes decide.
// shortcut: a broken file with the bytes `/Encrypt` gets EncryptedFile, not MalformedFile. Both
// fail the revision with no retry; read the message of pdftotext if the class must be exact.
pub fn pdftotext_failure(status: ExitStatus, bytes: &[u8]) -> i32 {
    match status.code() {
        Some(1 | 3) if bytes.windows(8).any(|w| w == b"/Encrypt") => exit::ENCRYPTED_FILE,
        Some(1 | 3 | 99) => exit::MALFORMED_FILE,
        // A stop on a signal has no code. The process is PID 1, so it reports the crash itself.
        _ => exit::CRASH,
    }
}

/// Reads the file from `input`, writes the JSON document to `output`, and gives the exit code.
pub fn run(pdftotext: &str, input: impl Read, mut output: impl Write) -> i32 {
    let mut bytes = Vec::new();
    if input.take(MAX_INPUT + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_INPUT
    {
        return exit::CRASH;
    }
    if detect(&bytes) == Format::Unsupported {
        return exit::UNSUPPORTED_FORMAT;
    }
    let Ok(mut child) = Command::new(pdftotext)
        .args(["-q", "-enc", "UTF-8", "-eol", "unix", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return exit::CRASH;
    };
    // A parser that stops early closes the pipe: the error of the write is not the result.
    let _ = child.stdin.take().map(|mut stdin| stdin.write_all(&bytes));
    let mut text = Vec::new();
    let read = child
        .stdout
        .take()
        .map(|out| out.take(MAX_OUTPUT + 1).read_to_end(&mut text));
    if text.len() as u64 > MAX_OUTPUT {
        let _ = child.kill();
        let _ = child.wait();
        return exit::OUTPUT_TOO_LARGE;
    }
    let status = match child.wait() {
        Ok(status) => status,
        Err(_) => return exit::CRASH,
    };
    if !status.success() {
        return pdftotext_failure(status, &bytes);
    }
    if !matches!(read, Some(Ok(_))) {
        return exit::CRASH;
    }
    let document = pdf_document(&String::from_utf8_lossy(&text));
    if serde_json::to_writer(&mut output, &document).is_err() || output.flush().is_err() {
        return exit::CRASH;
    }
    exit::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn the_format_comes_from_the_first_bytes() {
        assert_eq!(detect(b"%PDF-1.7\n"), Format::Pdf);
        assert_eq!(detect(b"PK\x03\x04"), Format::Unsupported);
        assert_eq!(detect(b" %PDF-1.7"), Format::Unsupported);
        assert_eq!(detect(b""), Format::Unsupported);
    }

    #[test]
    fn one_block_for_each_page_with_text() {
        let document = pdf_document("alpha\n\n\u{c}\n\u{c}bravo\n\u{c}");
        assert_eq!(
            document,
            serde_json::json!({"format": "pdf", "pages": 3, "blocks": [
                {"kind": "page", "index": 1, "text": "alpha"},
                {"kind": "page", "index": 3, "text": "bravo"},
            ]})
        );
    }

    #[test]
    fn a_text_loses_its_nul_characters() {
        let document = pdf_document("a\0b\u{c}\0\u{c}");
        assert_eq!(
            document["blocks"],
            serde_json::json!([{"kind": "page", "index": 1, "text": "ab"}])
        );
    }

    #[test]
    fn a_pdf_with_no_text_gives_no_blocks() {
        assert_eq!(
            pdf_document("\n\u{c}"),
            serde_json::json!({"format": "pdf", "pages": 1, "blocks": []})
        );
    }

    #[test]
    fn a_format_that_is_not_pdf_starts_no_parser() {
        let mut out = Vec::new();
        assert_eq!(
            run(
                "/nonexistent/pdftotext",
                &b"PK\x03\x04 report.pdf"[..],
                &mut out
            ),
            exit::UNSUPPORTED_FORMAT
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_parser_that_does_not_start_is_a_crash() {
        assert_eq!(
            run("/nonexistent/pdftotext", &b"%PDF-1.7"[..], Vec::new()),
            exit::CRASH
        );
    }

    #[test]
    fn an_input_above_the_limit_is_a_crash() {
        let input = io::repeat(b'%').take(MAX_INPUT + 1);
        assert_eq!(
            run("/nonexistent/pdftotext", input, Vec::new()),
            exit::CRASH
        );
    }

    #[test]
    fn each_exit_code_is_below_128() {
        for code in [
            exit::OK,
            exit::UNSUPPORTED_FORMAT,
            exit::ENCRYPTED_FILE,
            exit::MALFORMED_FILE,
            exit::OUTPUT_TOO_LARGE,
            exit::CRASH,
        ] {
            assert!((0..128).contains(&code));
        }
    }
}
