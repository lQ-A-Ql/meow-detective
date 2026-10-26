use app_services::{case_service, file_service, source_db};
use domain::{CaseId, DataSourceId, EntryType};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{collections::BTreeSet, io::Write, path::Path};
use transport::commands::{GetFileJumpContextRequest, GetFileRowsRequest};

fn scoped(source: &DataSourceId, local: &str) -> String {
    source_db::GlobalFileId::new(source.clone(), domain::FileEntryId(local.into()))
        .encode()
        .0
}

fn rows(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    parent: &str,
    offset: u64,
    hidden: bool,
) -> transport::dto::FileRowsPageDto {
    file_service::get_file_rows_for_case(
        conn,
        root,
        case,
        &GetFileRowsRequest {
            parent_id: Some(parent.into()),
            offset,
            show_hidden: hidden,
            ..Default::default()
        },
    )
    .unwrap()
}

fn context(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    id: &str,
    hidden: bool,
) -> transport::dto::FileJumpContextDto {
    file_service::get_file_jump_context_for_case(
        conn,
        root,
        case,
        &GetFileJumpContextRequest {
            file_id: id.into(),
            show_hidden: hidden,
            page_limit: 500,
            sort_key: Default::default(),
            sort_direction: Default::default(),
        },
    )
    .unwrap()
}

fn snapshot(conn: &Connection, root: &Path, case: &CaseId) -> Value {
    let sources = file_service::get_data_sources_for_case(conn, root, case).unwrap();
    let before = sources
        .iter()
        .map(|source| std::fs::metadata(&source.source_path).unwrap())
        .collect::<Vec<_>>();
    let source_connections =
        source_db::open_ready_source_connections_read_only(conn, root, case).unwrap();
    assert_eq!(source_connections.len(), 1);
    let (source_id, source_conn) = &source_connections[0];
    let repo = FileRepo::new(source_conn);
    let mut ids = BTreeSet::new();
    let lookup = |path: &str| {
        repo.find_by_partition_and_path(source_id, 4, path)
            .unwrap()
            .unwrap()
    };
    for path in [
        "",
        "usr",
        "usr/share",
        "usr/share/doc",
        "var",
        "var/lib",
        "var/lib/dpkg",
        "var/lib/dpkg/info",
        "etc",
    ] {
        ids.insert(scoped(source_id, &lookup(path).id.0));
    }
    let directory_id = scoped(source_id, &lookup("usr/share/doc").id.0);
    let listing_id = scoped(source_id, &lookup("var/lib/dpkg/info").id.0);
    let hidden_id = scoped(source_id, &lookup("etc/.updated").id.0);
    let off_page = rows(conn, root, case, &directory_id, 500, true);
    let target = off_page
        .rows
        .iter()
        .find(|row| row.entry_type == "directory")
        .unwrap();
    let off_page_id = target.id.clone();
    assert!(off_page.total_count > 500);
    ids.insert(off_page_id.clone());
    let listing_page = rows(conn, root, case, &listing_id, 0, true);
    let file = listing_page
        .rows
        .iter()
        .find(|row| row.entry_type == "file")
        .unwrap();
    let file_id = file.id.clone();
    ids.insert(file_id.clone());
    let far_file = rows(conn, root, case, &listing_id, 500, true)
        .rows
        .into_iter()
        .find(|row| row.entry_type == "file")
        .unwrap();
    let far_file_id = far_file.id;
    ids.insert(far_file_id.clone());
    ids.insert(hidden_id.clone());
    let mut queries = Vec::new();
    for hidden in [false, true] {
        let roots = file_service::get_file_tree_for_case(conn, root, case, hidden).unwrap();
        queries.push(json!({"key":["files","tree",case.0,hidden],"data":roots}));
        for id in &ids {
            let location = context(conn, root, case, id, hidden);
            queries.push(json!({"key":["files","jump-context",case.0,id,hidden,500,"name","asc"],"data":location}));
            let entry = repo
                .find_by_id(
                    &source_db::GlobalFileId::parse(&domain::FileEntryId(id.clone()))
                        .unwrap()
                        .local_id,
                )
                .unwrap()
                .unwrap();
            if entry.entry_type != EntryType::Directory {
                continue;
            }
            for offset in if id == &directory_id || id == &listing_id {
                vec![0, 500]
            } else {
                vec![0]
            } {
                queries.push(
                    json!({"key":["files","rows-page",case.0,id,offset,500,hidden,"name","asc"],
                    "data":rows(conn,root,case,id,offset,hidden)}),
                );
                queries.push(json!({"key":["files","children-page",case.0,id,offset,500,hidden],
                    "data":file_service::get_file_children_for_case(conn,root,case,id,offset,500,hidden).unwrap()}));
            }
        }
    }
    for (source, before) in sources.iter().zip(before) {
        let after = std::fs::metadata(&source.source_path).unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    }
    json!({"caseId":case.0,"sources":sources,"queries":queries,
        "rootId":scoped(source_id,&lookup("").id.0),"directoryId":directory_id,
        "offPageId":off_page_id,"listingId":listing_id,"fileId":file_id,"farFileId":far_file_id,"hiddenId":hidden_id,
        "hiddenParentId":scoped(source_id,&lookup("etc").id.0)})
}

#[test]
#[ignore = "requires FORENSICS_FILE_BROWSER_CASES and FORENSICS_FILE_BROWSER_OUTPUT"]
fn export_real_file_browser_service_snapshots() {
    let cases =
        std::env::var_os("FORENSICS_FILE_BROWSER_CASES").expect("set FORENSICS_FILE_BROWSER_CASES");
    let output = std::env::var_os("FORENSICS_FILE_BROWSER_OUTPUT")
        .expect("set FORENSICS_FILE_BROWSER_OUTPUT");
    let snapshots = std::env::split_paths(&cases)
        .map(|path| {
            let active = case_service::open_case(&path).unwrap();
            active
                .with_conn(|conn| Ok(snapshot(conn, &path, &active.meta.id)))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 2);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    file.write_all(&serde_json::to_vec(&snapshots).unwrap())
        .unwrap();
}
