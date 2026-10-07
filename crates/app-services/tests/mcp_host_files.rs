#[path = "support/mcp_files.rs"]
mod support;

use app_services::{case_service, file_service, mcp_host_service::McpHostServiceError};
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
use support::Fixture;

#[test]
fn file_listing_pages_roots_and_children_and_preserves_scoped_metadata() {
    let fixture = Fixture::new();
    let root = fixture
        .call("forensics.list_files", json!({"dataSourceId":"mcp-files"}))
        .unwrap();
    assert_eq!(root["rows"][0]["id"], fixture.id("evidence"));
    assert_eq!(root["totalCount"], 1);
    let mut names = Vec::new();
    for offset in 0..4 {
        let page = fixture.call("forensics.list_files", json!({"dataSourceId":"mcp-files", "parentId":fixture.id("evidence"), "limit":1, "offset":offset})).unwrap();
        assert_eq!(page["totalCount"], 4);
        assert_eq!(page["truncated"], offset < 3);
        let row = &page["rows"][0];
        assert_eq!(row["parentId"], fixture.id("evidence"));
        let metadata = fixture
            .call("forensics.get_file_metadata", json!({"fileId":row["id"]}))
            .unwrap();
        assert_eq!(metadata, *row);
        assert!(!metadata
            .to_string()
            .contains(&fixture.temporary.path().display().to_string()));
        names.push(row["name"].as_str().unwrap().to_string());
    }
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 4);
    let nested = fixture
        .call(
            "forensics.list_files",
            json!({"dataSourceId":"mcp-files", "parentId":fixture.id("nested")}),
        )
        .unwrap();
    assert_eq!(nested["rows"][0]["name"], "detail.txt");
    let past = fixture
        .call(
            "forensics.list_files",
            json!({"dataSourceId":"mcp-files", "parentId":fixture.id("evidence"), "offset":99}),
        )
        .unwrap();
    assert_eq!(past["rows"], json!([]));
    assert_eq!(past["truncated"], false);
    assert!(fixture
        .call(
            "forensics.list_files",
            json!({"dataSourceId":"mcp-files", "parentId":fixture.id("text.txt")})
        )
        .is_err());
    assert!(fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("nested")})
        )
        .is_err());
}

#[test]
fn real_reader_reassembles_binary_chunks_and_returns_exact_text_and_hashes() {
    let fixture = Fixture::new();
    let before = std::fs::read(fixture.temporary.path().join("evidence/binary.bin")).unwrap();
    let mut offset = 0;
    let mut recovered = Vec::new();
    loop {
        let chunk = fixture
            .call(
                "forensics.read_file",
                json!({"fileId":fixture.id("binary.bin"), "offset":offset}),
            )
            .unwrap();
        assert_eq!(chunk["encoding"], "base64");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk["content"].as_str().unwrap())
            .unwrap();
        assert!(bytes.len() <= 65_536);
        assert_eq!(chunk["bytesRead"], bytes.len());
        assert_eq!(chunk["chunkSha256"], hex::encode(Sha256::digest(&bytes)));
        recovered.extend(bytes);
        offset = chunk["nextOffset"].as_u64().unwrap();
        if chunk["eof"] == true {
            break;
        }
    }
    assert_eq!(recovered, fixture.binary);
    assert_eq!(offset, fixture.binary.len() as u64);
    assert_eq!(
        before,
        std::fs::read(fixture.temporary.path().join("evidence/binary.bin")).unwrap()
    );
    let text = fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt")}),
        )
        .unwrap();
    assert_eq!(text["encoding"], "utf8");
    assert_eq!(text["content"], "证据内容\nHello MCP\n");
    assert_eq!(text["eof"], true);
    let explicit = fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt"), "offset":13, "length":5, "encoding":"utf8"}),
        )
        .unwrap();
    assert_eq!(explicit["content"], "Hello");
    let encoded = fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt"), "encoding":"base64"}),
        )
        .unwrap();
    assert_eq!(encoded["encoding"], "base64");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded["content"].as_str().unwrap())
            .unwrap(),
        "证据内容\nHello MCP\n".as_bytes()
    );
}

#[test]
fn eof_empty_files_and_split_utf8_have_lossless_behavior_and_no_leaked_handles() {
    let fixture = Fixture::new();
    for (name, offset) in [
        ("empty.txt", 0),
        ("binary.bin", fixture.binary.len() as u64),
    ] {
        let result = fixture
            .call(
                "forensics.read_file",
                json!({"fileId":fixture.id(name), "offset":offset}),
            )
            .unwrap();
        assert_eq!(result["bytesRead"], 0);
        assert_eq!(result["content"], "");
        assert_eq!(result["eof"], true);
        assert_eq!(result["nextOffset"], offset);
    }
    assert!(matches!(
        fixture.call(
            "forensics.read_file",
            json!({"fileId":fixture.id("binary.bin"), "offset":fixture.binary.len() + 1})
        ),
        Err(McpHostServiceError::InvalidInput)
    ));
    assert!(matches!(
        fixture.call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt"), "length":2, "encoding":"utf8"})
        ),
        Err(McpHostServiceError::InvalidUtf8)
    ));
    let split = fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt"), "length":2}),
        )
        .unwrap();
    assert_eq!(split["encoding"], "base64");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(split["content"].as_str().unwrap())
            .unwrap(),
        &"证据内容".as_bytes()[..2]
    );
    std::fs::write(fixture.temporary.path().join("evidence/text.txt"), b"short").unwrap();
    assert!(matches!(
        fixture.call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt")})
        ),
        Err(McpHostServiceError::IncompleteRead)
    ));
    std::fs::remove_file(fixture.temporary.path().join("evidence/text.txt")).unwrap();
    assert!(fixture
        .call(
            "forensics.read_file",
            json!({"fileId":fixture.id("text.txt")})
        )
        .is_err());
}

#[test]
fn input_limits_host_paths_and_case_source_ownership_are_enforced() {
    let fixture = Fixture::new();
    for extra in [
        json!({"length":0}),
        json!({"length":65537}),
        json!({"offset":-1}),
        json!({"offset":1.5}),
        json!({"offset":9_007_199_254_740_992u64}),
        json!({"encoding":"gbk"}),
        json!({"path":"C:\\private.txt"}),
        json!({"caseRoot":"arbitrary"}),
    ] {
        let mut args = json!({"fileId":fixture.id("text.txt")});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(matches!(
            fixture.call("forensics.read_file", args),
            Err(McpHostServiceError::InvalidInput)
        ));
    }
    for extra in [
        json!({"limit":0}),
        json!({"limit":201}),
        json!({"parentId":null}),
        json!({"offset":-1}),
    ] {
        let mut args = json!({"dataSourceId":"mcp-files"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(fixture.call("forensics.list_files", args).is_err());
    }
    for id in ["text.txt", "C:\\private.txt", "ds:outside-case:missing"] {
        assert!(fixture
            .call("forensics.get_file_metadata", json!({"fileId":id}))
            .is_err());
        assert!(fixture
            .call("forensics.read_file", json!({"fileId":id}))
            .is_err());
    }
    assert!(fixture
        .call(
            "forensics.list_files",
            json!({"dataSourceId":"outside-case", "parentId":fixture.id("evidence")})
        )
        .is_err());
    let second =
        case_service::create_case(&fixture.temporary.path().join("cases"), "Other Case", None)
            .unwrap();
    assert!(file_service::get_file_metadata_for_case(
        &second.connection().unwrap(),
        &second.case_root,
        &second.meta.id,
        fixture.id("text.txt")
    )
    .is_err());
}
