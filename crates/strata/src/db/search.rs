//! The query of the search route (tech design 0049).

use tracing::Instrument;
use uuid::Uuid;

use super::{Error, Params, TenantTx};

/// The best part of each location (document, kind, number) of the tenant that matches the
/// query, best first, up to the limit (tech design 0145). The match uses the GIN index on
/// `(tenant_id, tsv)`. Only the keys and the ranks go through the limit: `ts_headline` reads the
/// text of the kept parts only. The query removes U+0002 and U+0003 from the text first, so the
/// marks of a matched word come from `ts_headline` only.
const SEARCH: &str = "with best as (\
     select distinct on (p.document_id, p.location_kind, p.location_number) \
     p.document_id, p.revision, p.part, p.location_kind, p.location_number, \
     ts_rank(p.tsv, q.query) as rank \
     from text_parts p, websearch_to_tsquery('simple', $2) as q (query) \
     where p.tenant_id = $1 and p.tsv @@ q.query \
     order by p.document_id, p.location_kind, p.location_number, rank desc, p.part), \
     top as (select * from best \
     order by rank desc, document_id, location_kind, location_number limit $3) \
     select t.document_id, t.revision, t.location_kind, t.location_number, t.rank, \
     ts_headline('simple', translate(p.part_text, chr(2) || chr(3), ''), \
     websearch_to_tsquery('simple', $2), \
     'StartSel=' || chr(2) || ', StopSel=' || chr(3) || ', MinWords=15, MaxWords=35') \
     from top t join text_parts p on (p.tenant_id, p.document_id, p.revision, p.part) \
     = ($1, t.document_id, t.revision, t.part) \
     order by t.rank desc, t.document_id, t.location_kind, t.location_number";

/// One hit: the location, the revision of its text, the rank of its best part, and the output
/// of `ts_headline` for that part, with U+0002 and U+0003 around each matched word.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub document_id: Uuid,
    pub revision: i32,
    pub score: f32,
    pub location_kind: String,
    pub location_number: i32,
    pub snippet: String,
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
            snippet: r.get(5),
        })
        .collect())
}
