//! The extraction of one file in the sandbox of tech design 0007, and the check of its output.

use serde::Deserialize;
use strata_core::revision::FailureClass;
use strata_extract::exit;

use crate::sandbox::{Run, Stop};

/// The maximum size of a text part (0049). A `tsvector` of this size stays far below the limits
/// of PostgreSQL.
pub const MAX_PART: usize = 64 * 1024;

/// One text part of a revision: the location of its block, and at most `MAX_PART` bytes of text.
#[derive(Debug, PartialEq, Eq)]
pub struct Part {
    /// The page, from 1.
    pub location: u32,
    pub text: String,
}

/// The result of one run, for the transaction that ends the job.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The text of the file. No part is the class `NoText` (0007).
    Text(Vec<Part>),
    /// A class of 0007 for the revision.
    Failed(FailureClass),
    /// nsjail could not start the sandbox: a normal failure of the job, with no change to the
    /// revision (owner, 2026-11-22).
    SandboxFailure,
    /// The lease is lost: nothing writes a result.
    Cancelled,
}

/// The map from each stop of a run to its result (0007, and the task 1.2 of #49).
pub fn outcome(run: Run) -> Outcome {
    use FailureClass::*;
    match run.stop {
        Stop::Memory => Outcome::Failed(ExtractionMemory),
        Stop::OutputTooLarge => Outcome::Failed(ExtractionOutputTooLarge),
        Stop::Timeout => Outcome::Failed(ExtractionTimeout),
        Stop::Cancelled => Outcome::Cancelled,
        Stop::SandboxFailure => Outcome::SandboxFailure,
        Stop::Crash => Outcome::Failed(ExtractionCrash),
        Stop::Exit(exit::OK) => match check(&run.output) {
            Some(parts) => Outcome::Text(parts),
            None => Outcome::Failed(InvalidOutput),
        },
        Stop::Exit(exit::UNSUPPORTED_FORMAT) => Outcome::Failed(UnsupportedFormat),
        Stop::Exit(exit::ENCRYPTED_FILE) => Outcome::Failed(EncryptedFile),
        Stop::Exit(exit::MALFORMED_FILE) => Outcome::Failed(MalformedFile),
        Stop::Exit(exit::OUTPUT_TOO_LARGE) => Outcome::Failed(ExtractionOutputTooLarge),
        // `exit::CRASH`, and each code that `strata-extract` does not give.
        Stop::Exit(_) => Outcome::Failed(ExtractionCrash),
    }
}

/// The output of `strata-extract` (0049). Each field that is not in the list fails the check.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    format: String,
    pages: u32,
    blocks: Vec<Block>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Block {
    kind: String,
    index: u32,
    text: String,
}

/// The check of E6 of 0007: the output is untrusted input. It must be JSON with the fields of the
/// format, valid UTF-8 (`serde_json` rejects other bytes), and each block a page of the file, in
/// order, with no NUL character. `None` is the result `InvalidOutput`. The caller checks the size.
pub fn check(output: &[u8]) -> Option<Vec<Part>> {
    let output: Output = serde_json::from_slice(output).ok()?;
    if output.format != "pdf" {
        return None;
    }
    let mut last = 0;
    let mut parts = Vec::new();
    for block in output.blocks {
        let in_order = last < block.index && block.index <= output.pages;
        if block.kind != "page" || !in_order || block.text.contains('\0') {
            return None;
        }
        last = block.index;
        parts.extend(split(&block.text).into_iter().map(|text| Part {
            location: block.index,
            text: text.to_owned(),
        }));
    }
    Some(parts)
}

/// The parts of a block (0049): a cut at the last whitespace before `MAX_PART`, or at the last
/// start of a UTF-8 character before `MAX_PART` when the block has no whitespace there. The parts,
/// in order, are the text of the block.
pub fn split(mut text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    while text.len() > MAX_PART {
        let head = &text[..text.floor_char_boundary(MAX_PART)];
        let cut = head
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(head.len(), |(i, c)| i + c.len_utf8());
        let (part, rest) = text.split_at(cut);
        parts.push(part);
        text = rest;
    }
    if !text.is_empty() {
        parts.push(text);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(stop: Stop, output: &[u8]) -> Outcome {
        outcome(Run {
            stop,
            output: output.to_vec(),
        })
    }

    #[test]
    fn each_stop_gives_one_result() {
        use FailureClass::*;
        let text = br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":1,"text":"a"}]}"#;
        assert_eq!(
            run(Stop::Exit(0), text),
            Outcome::Text(vec![Part {
                location: 1,
                text: "a".into()
            }])
        );
        assert_eq!(run(Stop::Exit(0), b"{}"), Outcome::Failed(InvalidOutput));
        // A memory stop wins over valid output (task 1.2 of #49).
        assert_eq!(run(Stop::Memory, text), Outcome::Failed(ExtractionMemory));
        let cases = [
            (
                Stop::OutputTooLarge,
                Outcome::Failed(ExtractionOutputTooLarge),
            ),
            (Stop::Timeout, Outcome::Failed(ExtractionTimeout)),
            (Stop::Crash, Outcome::Failed(ExtractionCrash)),
            (Stop::Cancelled, Outcome::Cancelled),
            (Stop::SandboxFailure, Outcome::SandboxFailure),
            (
                Stop::Exit(exit::UNSUPPORTED_FORMAT),
                Outcome::Failed(UnsupportedFormat),
            ),
            (
                Stop::Exit(exit::ENCRYPTED_FILE),
                Outcome::Failed(EncryptedFile),
            ),
            (
                Stop::Exit(exit::MALFORMED_FILE),
                Outcome::Failed(MalformedFile),
            ),
            (
                Stop::Exit(exit::OUTPUT_TOO_LARGE),
                Outcome::Failed(ExtractionOutputTooLarge),
            ),
            (Stop::Exit(exit::CRASH), Outcome::Failed(ExtractionCrash)),
            (Stop::Exit(1), Outcome::Failed(ExtractionCrash)),
        ];
        for (stop, expected) in cases {
            assert_eq!(run(stop, text), expected, "{stop:?}");
        }
    }

    fn invalid(output: &[u8]) {
        assert_eq!(check(output), None, "{}", String::from_utf8_lossy(output));
    }

    #[test]
    fn valid_output_gives_one_part_for_each_page() {
        let parts = check(br#"{"format":"pdf","pages":3,"blocks":[{"kind":"page","index":1,"text":"a"},{"kind":"page","index":3,"text":"c"}]}"#);
        assert_eq!(
            parts,
            Some(vec![
                Part {
                    location: 1,
                    text: "a".into()
                },
                Part {
                    location: 3,
                    text: "c".into()
                }
            ])
        );
        assert_eq!(
            check(br#"{"format":"pdf","pages":1,"blocks":[]}"#),
            Some(vec![])
        );
    }

    /// Scenario "Each form of output that fails the check".
    #[test]
    fn each_form_of_output_that_fails_the_check() {
        invalid(b"not json");
        invalid(b"");
        invalid(br#"{"format":"pdf","pages":1}"#);
        invalid(br#"{"format":"pdf","blocks":[]}"#);
        invalid(br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":1}]}"#);
        invalid(b"{\"format\":\"pdf\",\"pages\":1,\"blocks\":[{\"kind\":\"page\",\"index\":1,\"text\":\"\xff\xfe\"}]}");
        invalid(br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":0,"text":"a"}]}"#);
    }

    #[test]
    fn other_output_that_fails_the_check() {
        invalid(br#"{"format":"docx","pages":1,"blocks":[]}"#);
        invalid(br#"{"format":"pdf","pages":1,"blocks":[],"path":"/etc"}"#);
        invalid(br#"{"format":"pdf","pages":1,"blocks":[{"kind":"sheet","index":1,"text":"a"}]}"#);
        invalid(br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":2,"text":"a"}]}"#);
        invalid(br#"{"format":"pdf","pages":2,"blocks":[{"kind":"page","index":2,"text":"a"},{"kind":"page","index":1,"text":"b"}]}"#);
        invalid(
            br#"{"format":"pdf","pages":1,"blocks":[{"kind":"page","index":1,"text":"a\u0000"}]}"#,
        );
        invalid(br#"{"format":"pdf","pages":-1,"blocks":[]}"#);
    }

    /// Scenario "A page of 200 KB goes in four parts".
    #[test]
    fn a_page_of_200_kb_goes_in_four_parts() {
        let page = "word ".repeat(200 * 1024 / 5);
        let output = serde_json::json!({"format": "pdf", "pages": 1, "blocks": [{"kind": "page", "index": 1, "text": page}]});
        let parts = check(output.to_string().as_bytes()).unwrap();
        assert_eq!(parts.len(), 4);
        assert!(
            parts
                .iter()
                .all(|p| p.text.len() <= MAX_PART && p.location == 1)
        );
        assert!(parts[..3].iter().all(|p| p.text.ends_with(' ')));
        assert_eq!(
            parts.iter().map(|p| p.text.as_str()).collect::<String>(),
            page
        );
    }

    /// Scenario "A block with no whitespace".
    #[test]
    fn a_block_with_no_whitespace() {
        let block = "é".repeat(100 * 1024 / 2) + "a";
        let parts = split(&block);
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.len() <= MAX_PART));
        assert_eq!(parts.concat(), block);
    }

    #[test]
    fn a_block_of_64_kb_is_one_part() {
        let block = "a".repeat(MAX_PART);
        assert_eq!(split(&block), vec![block.as_str()]);
        assert!(split("").is_empty());
    }
}
