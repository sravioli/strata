//! Pure lifecycle of one document revision. No I/O, no async, no database.
//!
//! The state is one revision (`number`, `status`) plus the number of the
//! document's active revision as this revision sees it (`active`).
//! An event that does not apply leaves the state unchanged.

#![forbid(unsafe_code)]

/// Processing status of a revision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Uploaded,
    Processing,
    Ready,
    Failed,
    Deleted,
}

/// One revision and the document's active-revision pointer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Revision {
    /// Revision number. A larger number is a newer revision.
    pub number: u32,
    pub status: Status,
    /// Number of the document's active revision, if there is one.
    pub active: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    /// A worker took the revision: Uploaded -> Processing.
    StartProcessing,
    /// Indexing finished: Processing -> Ready.
    ProcessingSucceeded,
    /// Indexing failed: Processing -> Failed.
    ProcessingFailed,
    /// Make this revision the active one. Applies only if the revision is
    /// Ready and is newer than the current active revision.
    Activate,
    /// Another revision `n` of the same document became active. Applies only
    /// if `n` is not this revision and is newer than the current active
    /// revision.
    OtherActivated(u32),
    /// Delete the revision: any status -> Deleted.
    Delete,
}

/// A new revision: Uploaded, with the document's current active pointer.
pub fn new_revision(number: u32, active: Option<u32>) -> Revision {
    Revision {
        number,
        status: Status::Uploaded,
        active,
    }
}

/// True if `n` is newer than the active revision (or there is none).
pub fn newer_than_active(active: Option<u32>, n: u32) -> bool {
    match active {
        None => true,
        Some(a) => a < n,
    }
}

/// Only a Ready revision that is the active one is searchable.
pub fn searchable(r: Revision) -> bool {
    match r.status {
        Status::Ready => match r.active {
            None => false,
            Some(a) => a == r.number,
        },
        _ => false,
    }
}

/// The transition function.
pub fn step(r: Revision, e: Event) -> Revision {
    match e {
        Event::StartProcessing => match r.status {
            Status::Uploaded => Revision {
                status: Status::Processing,
                ..r
            },
            _ => r,
        },
        Event::ProcessingSucceeded => match r.status {
            Status::Processing => Revision {
                status: Status::Ready,
                ..r
            },
            _ => r,
        },
        Event::ProcessingFailed => match r.status {
            Status::Processing => Revision {
                status: Status::Failed,
                ..r
            },
            _ => r,
        },
        Event::Activate => match r.status {
            Status::Ready => {
                if newer_than_active(r.active, r.number) {
                    Revision {
                        active: Some(r.number),
                        ..r
                    }
                } else {
                    r
                }
            }
            _ => r,
        },
        Event::OtherActivated(n) => {
            if n == r.number {
                r
            } else if newer_than_active(r.active, n) {
                Revision {
                    active: Some(n),
                    ..r
                }
            } else {
                r
            }
        }
        Event::Delete => Revision {
            status: Status::Deleted,
            ..r
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: [Event; 9] = [
        Event::StartProcessing,
        Event::ProcessingSucceeded,
        Event::ProcessingFailed,
        Event::Activate,
        Event::OtherActivated(0),
        Event::OtherActivated(2),
        Event::OtherActivated(3),
        Event::OtherActivated(u32::MAX),
        Event::Delete,
    ];
    const STATUSES: [Status; 5] = [
        Status::Uploaded,
        Status::Processing,
        Status::Ready,
        Status::Failed,
        Status::Deleted,
    ];
    const ACTIVES: [Option<u32>; 5] = [None, Some(0), Some(2), Some(3), Some(u32::MAX)];

    fn all_states() -> Vec<Revision> {
        let mut v = Vec::new();
        for number in [0, 2, 3, u32::MAX] {
            for status in STATUSES {
                for active in ACTIVES {
                    v.push(Revision {
                        number,
                        status,
                        active,
                    });
                }
            }
        }
        v
    }

    #[test]
    fn happy_path_becomes_searchable() {
        let mut r = new_revision(2, Some(1));
        for e in [
            Event::StartProcessing,
            Event::ProcessingSucceeded,
            Event::Activate,
        ] {
            assert!(!searchable(r));
            r = step(r, e);
        }
        assert!(searchable(r));
        assert!(!searchable(step(r, Event::OtherActivated(3))));
        assert!(!searchable(step(r, Event::Delete)));
    }

    #[test]
    fn step_is_idempotent() {
        for r in all_states() {
            for e in EVENTS {
                assert_eq!(step(step(r, e), e), step(r, e));
            }
        }
    }

    #[test]
    fn active_pointer_never_goes_back() {
        for r in all_states() {
            for e in EVENTS {
                let after = step(r, e);
                match (r.active, after.active) {
                    (Some(a), Some(b)) => assert!(a <= b),
                    (Some(_), None) => panic!("active pointer was cleared"),
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn deleted_stays_deleted() {
        for r in all_states() {
            if r.status == Status::Deleted {
                for e in EVENTS {
                    let after = step(r, e);
                    assert_eq!(after.status, Status::Deleted);
                    assert!(!searchable(after));
                }
            }
        }
    }
}
