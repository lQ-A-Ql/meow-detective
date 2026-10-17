//! Explicit Release counterfactuals. Timing excludes SQL plans and correctness hashing.
use persistence_sqlite::repositories::{
    ntfs_staging_repo::NtfsStagingRepo as Repo, staging_repo::StagingRepo,
};
use std::time::Instant;

#[test]
#[ignore = "explicit Release cost experiment; set FORENSICS_NTFS_COST_RECORDS"]
fn parent_index_and_writer_cost() {
    let records: usize = std::env::var("FORENSICS_NTFS_COST_RECORDS")
        .expect("set records")
        .parse()
        .unwrap();
    assert!((32..=1_000_000).contains(&records));
    let mut results = Vec::new();
    for round in 0..5 {
        let order = if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        };
        for retained in order {
            let temp = tempfile::tempdir().unwrap();
            let c = StagingRepo::open_partition_staging_conn(temp.path(), "cost", 1).unwrap();
            for epoch in 0..2 {
                let tx = c.unchecked_transaction().unwrap();
                Repo::reset_layout(&tx).unwrap();
                Repo::begin(&tx, "cost", 1).unwrap();
                if retained && epoch == 1 {
                    tx.execute_batch(
                        "CREATE INDEX legacy_ingest_parent ON ntfs_entries(parent_record_num)",
                    )
                    .unwrap();
                }
                let started = Instant::now();
                let mut s=tx.prepare_cached("INSERT INTO ntfs_entries(record_num,parent_record_num,sequence_num,name,entry_type,size,ext,deleted,hidden,system,read_only,archive,encrypted,created_at,modified_at,accessed_at,changed_at) VALUES(?1,?2,7,?3,?4,12,'txt',0,0,0,0,0,0,NULL,NULL,NULL,NULL)").unwrap();
                for record in std::iter::once(0)
                    .chain(std::iter::once(5))
                    .chain(16..records)
                {
                    s.execute(rusqlite::params![
                        record,
                        if record == 5 { None } else { Some(5) },
                        format!("file-{record}.txt"),
                        i32::from(record == 5)
                    ])
                    .unwrap();
                }
                drop(s);
                let insert_ns = started.elapsed().as_nanos();
                let commit = Instant::now();
                tx.commit().unwrap();
                let commit_ns = commit.elapsed().as_nanos();
                let count: u64 = c
                    .query_row("SELECT COUNT(*) FROM ntfs_entries", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(count, records as u64 - 14);
                let schema:Vec<(String,String)>=c.prepare("SELECT name,sql FROM sqlite_master WHERE type IN ('table','index','view') AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
                let pages: u64 = c.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
                let page_size: u64 = c.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
                results.push(serde_json::json!({"round":round,"epoch":epoch,"retainedParent":retained,"rows":count,"insertNs":insert_ns,"commitNs":commit_ns,"logicalDatabaseBytes":pages*page_size,"schema":schema}));
            }
        }
    }
    eprintln!(
        "[ntfs-cost] {}",
        serde_json::json!({"version":1,"records":records,"scope":"counterfactual retained raw-parent index; native 17-column prepared writer; 5 alternating rounds; same partition transaction; no workflow speed claim","results":results})
    );
}
