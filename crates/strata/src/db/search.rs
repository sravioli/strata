//! The query of the search route (tech design 0049).

use tracing::Instrument;
use uuid::Uuid;

use super::{Error, Params, TenantTx};

/// The best part of each document of the tenant that matches the query, best first. The match
/// uses the GIN index on `(tenant_id, tsv)`.
const SEARCH: &str = "select document_id, revision, location_kind, location_number, rank \
     from (select distinct on (p.document_id) p.document_id, p.revision, p.location_kind, \
     p.location_number, ts_rank(p.tsv, q.query) as rank \
     from text_parts p, websearch_to_tsquery('simple', $2) as q (query) \
     where p.tenant_id = $1 and p.tsv @@ q.query \
     order by p.document_id, rank desc, p.part) as best \
     order by rank desc, document_id limit $3";

/// One result: the document, the revision of its best part, the rank, and the location.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub document_id: Uuid,
    pub revision: i32,
    pub score: f32,
    pub location_kind: String,
    pub location_number: i32,
}

/// The search in the tenant transaction of the scope. The statement timeout, the query and the
/// `commit` go together (the pipelining rule of 0049). A timeout gives an error for which
/// `Error::is_statement_timeout` is true.
pub async fn search(tx: TenantTx, q: &str, limit: i64, timeout_ms: u64) -> Result<Vec<Hit>, Error> {
    let tenant = tx.tenant_id;
    let rows = {
        let c = tx.tx.conn();
        let st = c.prepare_cached(SEARCH).await?;
        let set = format!("set local statement_timeout = {timeout_ms}");
        let p: Params = &[&tenant, &q, &limit];
        let ((), rows, ()) = tokio::try_join!(
            c.batch_execute(&set),
            c.query(&st, p).instrument(crate::db::span("search")),
            c.batch_execute("commit")
        )?;
        rows
    };
    tx.tx.release();
    Ok(rows
        .iter()
        .map(|r| Hit {
            document_id: r.get(0),
            revision: r.get(1),
            location_kind: r.get(2),
            location_number: r.get(3),
            score: r.get(4),
        })
        .collect())
}
