use super::*;
use serde_json::json;

#[test]
fn host_contract_round_trips_camel_case_and_omits_absent_errors() {
    let settings = McpHostSettingsDto {
        enabled: true,
        disabled_tools: vec!["forensics.get_current_case".into()],
    };
    let status = McpHostStatusDto {
        settings,
        running: true,
        endpoint: "http://127.0.0.1:3001/mcp".into(),
        last_error: None,
    };
    let value = serde_json::to_value(status).unwrap();
    assert_eq!(
        value["settings"]["disabledTools"][0],
        "forensics.get_current_case"
    );
    assert!(value.get("lastError").is_none());
    let decoded: McpHostStatusDto = serde_json::from_value(value).unwrap();
    assert!(decoded.running);
    assert!(McpHostSettingsDto::default().enabled);
    assert!(
        serde_json::from_value::<McpHostSettingsDto>(json!({"enabled":true,"host":"0.0.0.0"}))
            .is_err()
    );
}

#[test]
fn tools_and_calls_preserve_schema_and_arguments() {
    let tool = McpHostToolDto {
        name: "forensics.get_data_source".into(),
        description: "Read source".into(),
        input_schema: json!({"type":"object"}),
    };
    let value = serde_json::to_value(tool).unwrap();
    assert!(value.get("inputSchema").is_some());
    let _: McpHostToolDto = serde_json::from_value(value).unwrap();
    let request: McpHostToolCallRequestDto =
        serde_json::from_value(json!({"name":"forensics.list_data_sources"})).unwrap();
    assert_eq!(request.arguments, json!({}));
    let request_value = serde_json::to_value(request).unwrap();
    let _: McpHostToolCallRequestDto = serde_json::from_value(request_value).unwrap();
    let result = McpHostToolCallResultDto {
        success: true,
        data: Some(json!([])),
        error: None,
    };
    let value = serde_json::to_value(result).unwrap();
    assert!(value.get("error").is_none());
    assert!(
        serde_json::from_value::<McpHostToolCallResultDto>(value)
            .unwrap()
            .success
    );
}

#[test]
fn file_requests_and_chunks_round_trip_defaults_encodings_and_camel_case() {
    let list: McpHostListFilesRequestDto =
        serde_json::from_value(json!({"dataSourceId":"source"})).unwrap();
    assert_eq!(list.limit, 100);
    assert_eq!(list.offset, 0);
    let value = serde_json::to_value(list).unwrap();
    assert!(value.get("parentId").is_none());
    let _: McpHostListFilesRequestDto = serde_json::from_value(value).unwrap();
    let metadata: McpHostFileRequestDto =
        serde_json::from_value(json!({"fileId":"ds:source:file"})).unwrap();
    let _: McpHostFileRequestDto =
        serde_json::from_value(serde_json::to_value(metadata).unwrap()).unwrap();
    let read: McpHostReadFileRequestDto =
        serde_json::from_value(json!({"fileId":"ds:source:file"})).unwrap();
    assert_eq!(read.length, MCP_FILE_CHUNK_LIMIT);
    assert_eq!(read.encoding, McpHostFileEncodingDto::Auto);
    let _: McpHostReadFileRequestDto =
        serde_json::from_value(serde_json::to_value(read).unwrap()).unwrap();
    for encoding in [
        McpHostFileEncodingDto::Auto,
        McpHostFileEncodingDto::Utf8,
        McpHostFileEncodingDto::Base64,
    ] {
        let encoded = serde_json::to_value(encoding).unwrap();
        assert_eq!(
            serde_json::from_value::<McpHostFileEncodingDto>(encoded).unwrap(),
            encoding
        );
    }
    assert!(serde_json::from_value::<McpHostReadFileRequestDto>(
        json!({"fileId":"x", "path":"private"})
    )
    .is_err());
    let chunk = McpHostFileChunkDto {
        file_id: "ds:source:file".into(),
        size: 4,
        offset: 0,
        bytes_read: 4,
        next_offset: 4,
        eof: true,
        encoding: McpHostFileEncodingDto::Utf8,
        content: "test".into(),
        chunk_sha256: "hash".into(),
    };
    let value = serde_json::to_value(chunk).unwrap();
    assert_eq!(value["bytesRead"], 4);
    assert_eq!(value["nextOffset"], 4);
    assert_eq!(value["chunkSha256"], "hash");
    assert_eq!(
        serde_json::from_value::<McpHostFileChunkDto>(value)
            .unwrap()
            .content,
        "test"
    );
}
