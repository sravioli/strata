//! The queries of the upload routes (tech designs 0006 and 0049). Each takes the tenant
//! transaction, and each step is one statement (the batching rule of 0049).

use strata_core::revision::{FailureClass, Revision, RevisionState};
use uuid::Uuid;

use super::{Error, Params, TenantTx};

/// The counts of the two bounds of a start: the `Waiting` jobs (0005), and the rows of
/// `upload_slots` from the last 2 hours (0006). Both tables are global.
pub const BOUND_COUNTS: &str = "select (select count(*) from jobs where state = 'Waiting'), \
     (select count(*) from upload_slots where created_at > now() - interval '2 hours')";

/// A new document with its revision 1. The row of `upload_slots` comes from the row of the
/// document, as in `START_REVISION`.
const START_DOCUMENT: &str = "with d as (insert into documents (tenant_id, last_revision) \
     values ($1, 1) returning document_id), \
     s as (insert into upload_slots (created_at) select now() from d returning slot_id) \
     insert into revisions (tenant_id, document_id, revision, file_name, content_type, size, slot_id) \
     select $1, d.document_id, 1, $2, $3, $4, s.slot_id from d, s returning document_id, revision";

/// The next revision of a document. The row lock of the update puts two starts in sequence, so
/// each gets its own number (0049). A document that the policy hides gives no row, and then no
/// row of `upload_slots` exists either.
const START_REVISION: &str = "with d as (update documents set last_revision = last_revision + 1 \
     where tenant_id = $1 and document_id = $5 returning document_id, last_revision), \
     s as (insert into upload_slots (created_at) select now() from d returning slot_id) \
     insert into revisions (tenant_id, document_id, revision, file_name, content_type, size, slot_id) \
     select $1, d.document_id, d.last_revision, $2, $3, $4, s.slot_id from d, s \
     returning document_id, revision";

const LOCK_REVISION: &str = "select state, retry_mark, error_class, no_text, slot_id \
     from revisions where tenant_id = $1 and document_id = $2 and revision = $3 for update";

/// The write of a finish that leaves `AwaitingUpload`: the result of `revision_step`, the removal
/// of the row of `upload_slots`, and the job when `$9` is true (0005). The Rust code decides `$9`.
/// `strata_api` can read only `state` of `jobs`, so the insert has no `returning` and no
/// `on conflict`.
pub const WRITE_FINISH: &str = "with u as (update revisions set state = $4, retry_mark = $5, \
     error_class = $6, no_text = $7, status_at = now() \
     where tenant_id = $1 and document_id = $2 and revision = $3), \
     s as (delete from upload_slots where slot_id = $8) \
     insert into jobs (tenant_id, kind, document_id, revision) \
     select $1, 'ProcessDocument', $2, $3 where $9";

/// What the client sent at the start (0006).
pub struct NewRevision<'a> {
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub size: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    pub document_id: Uuid,
    pub revision: i32,
}

/// A revision under the row lock of the finish.
pub struct Locked {
    pub revision: Revision,
    pub slot_id: Uuid,
}

pub async fn bound_counts(tx: &mut TenantTx) -> Result<(i64, i64), Error> {
    let c = tx.tx.conn();
    let st = c.prepare_cached(BOUND_COUNTS).await?;
    let row = c.query_one(&st, &[]).await?;
    Ok((row.get(0), row.get(1)))
}

fn started(rows: Vec<tokio_postgres::Row>) -> Option<Started> {
    rows.first().map(|row| Started {
        document_id: row.get(0),
        revision: row.get(1),
    })
}

/// Creates the document and its revision 1, and commits.
pub async fn start_document(tx: TenantTx, new: &NewRevision<'_>) -> Result<Started, Error> {
    let tenant = tx.tenant_id;
    let p: Params = &[&tenant, &new.file_name, &new.content_type, &new.size];
    let rows = tx.query_and_end(START_DOCUMENT, p, true).await?;
    Ok(started(rows).expect("the insert of a document returns its row"))
}

/// Creates the next revision, and commits. `None` when the tenant has no document with that
/// identifier: then the statement wrote nothing.
pub async fn start_revision(
    tx: TenantTx,
    document_id: Uuid,
    new: &NewRevision<'_>,
) -> Result<Option<Started>, Error> {
    let tenant = tx.tenant_id;
    let p: Params = &[
        &tenant,
        &new.file_name,
        &new.content_type,
        &new.size,
        &document_id,
    ];
    Ok(started(tx.query_and_end(START_REVISION, p, true).await?))
}

/// Locks the revision until the end of the transaction. `None` when the tenant has no such
/// revision.
pub async fn lock_revision(
    tx: &mut TenantTx,
    document_id: Uuid,
    revision: i32,
) -> Result<Option<Locked>, Error> {
    let tenant = tx.tenant_id;
    let c = tx.tx.conn();
    let st = c.prepare_cached(LOCK_REVISION).await?;
    let Some(row) = c
        .query_opt(&st, &[&tenant, &document_id, &revision])
        .await?
    else {
        return Ok(None);
    };
    let state: &str = row.get(0);
    let class: Option<&str> = row.get(2);
    Ok(Some(Locked {
        revision: Revision {
            state: parse_state(state),
            retry_mark: row.get(1),
            error_class: class.map(parse_class),
            no_text: row.get(3),
        },
        slot_id: row.get(4),
    }))
}

/// Writes the result of `revision_step` for a revision that leaves `AwaitingUpload`, and ends
/// the transaction with `commit` (the route) or `rollback` (a test of TC-5-01).
pub async fn write_finish(
    tx: TenantTx,
    document_id: Uuid,
    revision: i32,
    r: Revision,
    slot_id: Uuid,
    commit: bool,
) -> Result<(), Error> {
    let tenant = tx.tenant_id;
    let state = format!("{:?}", r.state);
    let class = r.error_class.map(|c| format!("{c:?}"));
    // Only the change from `AwaitingUpload` to `Uploaded` makes a job.
    let job = r.state == RevisionState::Uploaded;
    let p: Params = &[
        &tenant,
        &document_id,
        &revision,
        &state,
        &r.retry_mark,
        &class,
        &r.no_text,
        &slot_id,
        &job,
    ];
    tx.query_and_end(WRITE_FINISH, p, commit).await?;
    Ok(())
}

// The names in the database are the names of the Rust variants. The check constraints of V2
// hold the same lists, so another text cannot come back.
fn parse_state(s: &str) -> RevisionState {
    match s {
        "AwaitingUpload" => RevisionState::AwaitingUpload,
        "Uploaded" => RevisionState::Uploaded,
        "Ready" => RevisionState::Ready,
        "Failed" => RevisionState::Failed,
        "Superseded" => RevisionState::Superseded,
        other => unreachable!("a state that the check constraint refuses: {other}"),
    }
}

fn parse_class(s: &str) -> FailureClass {
    use FailureClass::*;
    match s {
        "FileTooLarge" => FileTooLarge,
        "ExtractionTimeout" => ExtractionTimeout,
        "ExtractionMemory" => ExtractionMemory,
        "ExtractionCrash" => ExtractionCrash,
        "ExtractionOutputTooLarge" => ExtractionOutputTooLarge,
        "ArchiveTooLarge" => ArchiveTooLarge,
        "UnsupportedFormat" => UnsupportedFormat,
        "EncryptedFile" => EncryptedFile,
        "MalformedFile" => MalformedFile,
        "InvalidOutput" => InvalidOutput,
        other => unreachable!("a class that the check constraint refuses: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_go_both_ways() {
        use RevisionState::*;
        for s in [AwaitingUpload, Uploaded, Ready, Failed, Superseded] {
            assert_eq!(parse_state(&format!("{s:?}")), s);
        }
        use FailureClass::*;
        for c in [
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
        ] {
            assert_eq!(parse_class(&format!("{c:?}")), c);
        }
    }
}
