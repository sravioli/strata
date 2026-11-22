//! The revision state function (tech design 0049, "The lifecycle of a revision"). The theorems of
//! its "Proof obligations" table are about `revision_step` and `may_extract`
//! (`lean/Strata/Proofs/Revision.lean`). The API and the worker write the result; SQL decides no
//! change. Proved subset of tech design 0009: enums, structs, `match` and comparison.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionState {
    AwaitingUpload,
    Uploaded,
    Ready,
    Failed,
    Superseded,
}

/// The class of a `Failed` revision: `FileTooLarge` of 0006, or a class of 0007.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    FileTooLarge,
    ExtractionTimeout,
    ExtractionMemory,
    ExtractionCrash,
    ExtractionOutputTooLarge,
    ArchiveTooLarge,
    UnsupportedFormat,
    EncryptedFile,
    MalformedFile,
    InvalidOutput,
}

/// The fields of a revision row that the state function reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revision {
    pub state: RevisionState,
    /// True after the first crash, time limit or invalid output (0007).
    pub retry_mark: bool,
    /// The class of a `Failed` revision. `None` in each other state.
    pub error_class: Option<FailureClass>,
    /// True when a `Ready` revision has no text (`NoText` of 0007).
    pub no_text: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The finish of an upload, with an object of the maximum size or less.
    Finish,
    /// The finish of an upload, with an object above the maximum size.
    FinishTooLarge,
    /// The extraction succeeded. The argument is `no_text`.
    Extracted(bool),
    ExtractFailed(FailureClass),
    /// A newer revision of the document became `Ready`.
    NewerReady,
}

/// The next revision. Each pair of a state and an event that the table of 0049 does not name
/// leaves the revision as it is. The parameter is `r`: in Lean, `revision` is the namespace of
/// this module.
pub fn revision_step(r: Revision, e: Event) -> Revision {
    match (r.state, e) {
        (RevisionState::AwaitingUpload, Event::Finish) => Revision {
            state: RevisionState::Uploaded,
            ..r
        },
        (RevisionState::AwaitingUpload, Event::FinishTooLarge) => Revision {
            state: RevisionState::Failed,
            error_class: Some(FailureClass::FileTooLarge),
            ..r
        },
        (RevisionState::Uploaded, Event::Extracted(no_text)) => Revision {
            state: RevisionState::Ready,
            no_text,
            ..r
        },
        (RevisionState::Uploaded, Event::ExtractFailed(c)) => match c {
            // A crash, the time limit or invalid output retries one time (0007).
            FailureClass::ExtractionCrash
            | FailureClass::ExtractionTimeout
            | FailureClass::InvalidOutput => {
                if r.retry_mark {
                    Revision {
                        state: RevisionState::Failed,
                        error_class: Some(c),
                        ..r
                    }
                } else {
                    Revision {
                        retry_mark: true,
                        ..r
                    }
                }
            }
            _ => Revision {
                state: RevisionState::Failed,
                error_class: Some(c),
                ..r
            },
        },
        (
            RevisionState::AwaitingUpload | RevisionState::Uploaded | RevisionState::Ready,
            Event::NewerReady,
        ) => Revision {
            state: RevisionState::Superseded,
            ..r
        },
        _ => r,
    }
}

/// True only for an `Uploaded` revision: the worker extracts only such a revision (0047).
pub fn may_extract(r: Revision) -> bool {
    matches!(r.state, RevisionState::Uploaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use FailureClass::*;
    use RevisionState::*;

    const STATES: [RevisionState; 5] = [AwaitingUpload, Uploaded, Ready, Failed, Superseded];
    const CLASSES: [FailureClass; 10] = [
        FileTooLarge,
        ExtractionTimeout,
        ExtractionMemory,
        ExtractionCrash,
        ExtractionOutputTooLarge,
        ArchiveTooLarge,
        UnsupportedFormat,
        EncryptedFile,
        MalformedFile,
        InvalidOutput,
    ];

    fn events() -> Vec<Event> {
        let mut e = vec![
            Event::Finish,
            Event::FinishTooLarge,
            Event::Extracted(false),
            Event::Extracted(true),
            Event::NewerReady,
        ];
        e.extend(CLASSES.map(Event::ExtractFailed));
        e
    }

    fn rev(state: RevisionState, retry_mark: bool) -> Revision {
        Revision {
            state,
            retry_mark,
            error_class: (state == Failed).then_some(MalformedFile),
            no_text: false,
        }
    }

    /// The table of 0049, row by row. `None` is a pair that the table does not name.
    fn table(r: Revision, e: Event) -> Option<Revision> {
        let failed = |c| Revision {
            state: Failed,
            error_class: Some(c),
            ..r
        };
        let retries = |c| matches!(c, ExtractionCrash | ExtractionTimeout | InvalidOutput);
        match e {
            Event::Finish if r.state == AwaitingUpload => Some(Revision {
                state: Uploaded,
                ..r
            }),
            Event::FinishTooLarge if r.state == AwaitingUpload => Some(failed(FileTooLarge)),
            Event::Extracted(no_text) if r.state == Uploaded => Some(Revision {
                state: Ready,
                no_text,
                ..r
            }),
            Event::ExtractFailed(c) if r.state == Uploaded && retries(c) && !r.retry_mark => {
                Some(Revision {
                    retry_mark: true,
                    ..r
                })
            }
            Event::ExtractFailed(c) if r.state == Uploaded => Some(failed(c)),
            Event::NewerReady if [AwaitingUpload, Uploaded, Ready].contains(&r.state) => {
                Some(Revision {
                    state: Superseded,
                    ..r
                })
            }
            _ => None,
        }
    }

    #[test]
    fn each_pair_of_a_state_and_an_event() {
        let mut named = 0;
        for state in STATES {
            for mark in [false, true] {
                let r = rev(state, mark);
                for e in events() {
                    let want = table(r, e);
                    named += usize::from(want.is_some());
                    assert_eq!(revision_step(r, e), want.unwrap_or(r), "{r:?} {e:?}");
                }
            }
        }
        // 2 finish rows, 2 extracted, 10 classes and 3 superseded, each with and without the mark.
        assert_eq!(named, 2 * (2 + 2 + 10 + 3));
    }

    #[test]
    fn a_retry_class_retries_one_time() {
        let r = revision_step(rev(Uploaded, false), Event::ExtractFailed(ExtractionCrash));
        assert_eq!(
            (r.state, r.retry_mark, r.error_class),
            (Uploaded, true, None)
        );
        let r = revision_step(r, Event::ExtractFailed(ExtractionCrash));
        assert_eq!((r.state, r.error_class), (Failed, Some(ExtractionCrash)));
    }

    #[test]
    fn extraction_only_of_an_uploaded_revision() {
        for state in STATES {
            assert_eq!(
                may_extract(rev(state, false)),
                state == Uploaded,
                "{state:?}"
            );
        }
    }
}
