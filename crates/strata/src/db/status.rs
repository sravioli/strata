//! The reads of the status routes (tech design 0047). Each read is one statement in the tenant
//! transaction of the decision, sent with its `commit` (the pipelining rule of 0049).

use uuid::Uuid;

use super::{Error, Params, TenantTx};
use crate::status::Status;

const READ_ONE: &str = "select state, retry_mark, error_class, no_text, \
     floor(extract(epoch from status_at))::bigint from revisions \
     where tenant_id = $1 and document_id = $2 and revision = $3";

/// The status of one revision, or `None` when the tenant has no such revision.
pub async fn read_one(
    tx: TenantTx,
    document_id: Uuid,
    revision: i32,
) -> Result<Option<Status>, Error> {
    let tenant = tx.tenant_id;
    let p: Params = &[&tenant, &document_id, &revision];
    let rows = tx.query_and_end(READ_ONE, p, true).await?;
    Ok(rows.first().map(|row| Status {
        document_id,
        revision,
        r: super::upload::revision_of(row),
        updated_at: row.get(4),
    }))
}

const READ_MANY: &str = "select r.state, r.retry_mark, r.error_class, r.no_text, \
     floor(extract(epoch from r.status_at))::bigint, r.document_id, r.revision from revisions r \
     join unnest($2::uuid[], $3::integer[]) with ordinality as i (document_id, revision, n) \
     on r.document_id = i.document_id and r.revision = i.revision \
     where r.tenant_id = $1 order by i.n";

/// The status of each revision of `ids` that the tenant has, in the order of `ids`. One
/// statement reads all of them.
pub async fn read_many(tx: TenantTx, ids: &[(Uuid, i32)]) -> Result<Vec<Status>, Error> {
    let tenant = tx.tenant_id;
    let (documents, revisions): (Vec<Uuid>, Vec<i32>) = ids.iter().copied().unzip();
    let p: Params = &[&tenant, &documents, &revisions];
    let rows = tx.query_and_end(READ_MANY, p, true).await?;
    Ok(rows
        .iter()
        .map(|row| Status {
            document_id: row.get(5),
            revision: row.get(6),
            r: super::upload::revision_of(row),
            updated_at: row.get(4),
        })
        .collect())
}
