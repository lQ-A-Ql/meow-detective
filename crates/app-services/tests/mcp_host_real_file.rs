//! Opt-in read-only MCP regression against an already imported private case.
use std::{path::PathBuf, sync::Arc};

use app_services::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::PreviewRuntimeRegistry,
    mcp_host_service::{execute_file_tool, McpHostFileQueryContext, McpHostQueryContext},
    source_db::GlobalFileId,
};
use base64::Engine;
use domain::{CaseMeta, FileEntryId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[test]
#[ignore = "requires an explicitly selected private case, file and expected prefix"]
fn imported_e01_file_is_read_losslessly_through_mcp_in_bounded_chunks() {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("set FORENSICS_MCP_CASE_ROOT"),
    );
    let file_id = std::env::var("FORENSICS_MCP_FILE_ID").expect("set FORENSICS_MCP_FILE_ID");
    let prefix = hex::decode(
        std::env::var("FORENSICS_MCP_EXPECTED_PREFIX_HEX")
            .expect("set FORENSICS_MCP_EXPECTED_PREFIX_HEX"),
    )
    .unwrap();
    assert!(
        !prefix.is_empty(),
        "An independent prefix oracle is required"
    );
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let global = GlobalFileId::parse(&FileEntryId(file_id.clone())).unwrap();
    let source = DataSourceRepo::new(&conn)
        .find_by_case(&meta.id)
        .unwrap()
        .into_iter()
        .find(|source| source.id == global.data_source_id)
        .unwrap();
    assert_eq!(source.kind, domain::DataSourceKind::E01);
    let before = std::fs::metadata(&source.source_path).unwrap();
    let registry = PreviewRuntimeRegistry::default();
    let bitlocker = Arc::new(BitLockerUnlockRegistry::default());
    let call = |name, args: Value| {
        execute_file_tool(
            McpHostFileQueryContext {
                query: McpHostQueryContext {
                    connection: &conn,
                    case_root: &root,
                    case_meta: &meta,
                },
                preview_runtime: &registry,
                bitlocker_runtime: &bitlocker,
            },
            name,
            &args,
        )
        .unwrap()
    };
    let metadata = call("forensics.get_file_metadata", json!({"fileId":file_id}));
    let size = metadata["size"].as_u64().unwrap();
    assert!(
        size > 0 && size <= 8 * 1024 * 1024,
        "Select a nonempty file up to 8 MiB"
    );
    let mut content = Vec::new();
    let mut chunks = 0;
    loop {
        let chunk = call(
            "forensics.read_file",
            json!({"fileId":file_id, "offset":content.len(), "encoding":"base64"}),
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk["content"].as_str().unwrap())
            .unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= 65536);
        assert_eq!(chunk["bytesRead"], bytes.len());
        assert_eq!(chunk["chunkSha256"], hex::encode(Sha256::digest(&bytes)));
        content.extend(bytes);
        chunks += 1;
        assert_eq!(chunk["nextOffset"], content.len());
        assert_eq!(registry.stats().unwrap().session_count, 0);
        if chunk["eof"] == true {
            break;
        }
        assert!((content.len() as u64) < size);
    }
    assert_eq!(content.len() as u64, size);
    assert!(
        content.starts_with(&prefix),
        "Evidence prefix must match the independent oracle; actual first 16 bytes={}",
        hex::encode(&content[..content.len().min(16)])
    );
    let eof = call(
        "forensics.read_file",
        json!({"fileId":file_id, "offset":size}),
    );
    assert_eq!(eof["bytesRead"], 0);
    assert_eq!(eof["eof"], true);
    let digest = hex::encode(Sha256::digest(&content));
    if let Some(expected) = metadata["hashSha256"].as_str() {
        assert_eq!(digest, expected.to_lowercase());
    }
    let after = std::fs::metadata(&source.source_path).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    println!("E01 MCP file read passed: bytes={size}, chunks={chunks}, sha256={digest}, prefix_oracle=true, handles=0");
}

#[test]
#[ignore = "requires an explicitly selected private case and WOF-backed file"]
fn imported_wof_file_reports_unsupported_instead_of_successful_zero_bytes() {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_MCP_CASE_ROOT").expect("set FORENSICS_MCP_CASE_ROOT"),
    );
    let file_id =
        std::env::var("FORENSICS_MCP_WOF_FILE_ID").expect("set FORENSICS_MCP_WOF_FILE_ID");
    let meta: CaseMeta =
        serde_json::from_slice(&std::fs::read(root.join("case.json")).unwrap()).unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let registry = PreviewRuntimeRegistry::default();
    let bitlocker = Arc::new(BitLockerUnlockRegistry::default());
    let result = execute_file_tool(
        McpHostFileQueryContext {
            query: McpHostQueryContext {
                connection: &conn,
                case_root: &root,
                case_meta: &meta,
            },
            preview_runtime: &registry,
            bitlocker_runtime: &bitlocker,
        },
        "forensics.read_file",
        &json!({"fileId":file_id}),
    );
    assert!(matches!(
        result,
        Err(app_services::mcp_host_service::McpHostServiceError::UnsupportedFile)
    ));
    assert_eq!(registry.stats().unwrap().session_count, 0);
    println!("WOF MCP read rejected with typed Unsupported; handles=0");
}
