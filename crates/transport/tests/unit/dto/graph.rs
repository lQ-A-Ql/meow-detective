use super::*;

#[test]
fn graph_node_dto_serializes_camel_case() {
    let node = GraphNodeDto {
        id: "node-1".to_string(),
        case_id: "case-1".to_string(),
        node_type: GraphNodeTypeDto::File,
        label: "cmd.exe".to_string(),
        summary: "Command Prompt executable".to_string(),
        tags: vec!["executable".to_string(), "system".to_string()],
        created_at: "2026-06-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(node).unwrap();

    assert_eq!(json["id"], "node-1");
    assert_eq!(json["caseId"], "case-1");
    assert_eq!(json["nodeType"], "file");
    assert_eq!(json["label"], "cmd.exe");
    assert_eq!(json["summary"], "Command Prompt executable");
    assert_eq!(json["tags"][0], "executable");
    assert_eq!(json["tags"][1], "system");
    assert_eq!(json["createdAt"], "2026-06-14T00:00:00Z");
    // Ensure snake_case keys are absent
    assert!(json.get("case_id").is_none());
    assert!(json.get("node_type").is_none());
    assert!(json.get("created_at").is_none());
}

#[test]
fn graph_edge_dto_serializes_camel_case() {
    let edge = GraphEdgeDto {
        id: "edge-1".to_string(),
        case_id: "case-1".to_string(),
        source_id: "node-1".to_string(),
        target_id: "node-2".to_string(),
        edge_type: GraphEdgeTypeDto::References,
        confidence: Some(0.95),
        provenance: None,
        created_at: "2026-06-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(edge).unwrap();

    assert_eq!(json["id"], "edge-1");
    assert_eq!(json["caseId"], "case-1");
    assert_eq!(json["sourceId"], "node-1");
    assert_eq!(json["targetId"], "node-2");
    assert_eq!(json["edgeType"], "references");
    assert_eq!(json["confidence"], 0.95);
    assert_eq!(json["createdAt"], "2026-06-14T00:00:00Z");
    // provenance is None, should be absent
    assert!(json.get("provenance").is_none());
    // Ensure snake_case keys are absent
    assert!(json.get("case_id").is_none());
    assert!(json.get("source_id").is_none());
    assert!(json.get("target_id").is_none());
    assert!(json.get("edge_type").is_none());
}

#[test]
fn graph_edge_dto_omits_none_confidence() {
    let edge = GraphEdgeDto {
        id: "edge-2".to_string(),
        case_id: "case-1".to_string(),
        source_id: "node-1".to_string(),
        target_id: "node-3".to_string(),
        edge_type: GraphEdgeTypeDto::CorrelatesWith,
        confidence: None,
        provenance: Some(r#"{"source":"artifact-1"}"#.to_string()),
        created_at: "2026-06-14T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(edge).unwrap();

    assert!(json.get("confidence").is_none());
    assert_eq!(json["provenance"], r#"{"source":"artifact-1"}"#);
}

#[test]
fn graph_query_dto_defaults_and_camel_case() {
    let query = GraphQueryDto {
        start_ids: vec!["node-1".to_string()],
        edge_types: vec!["references".to_string(), "contains".to_string()],
        max_depth: 3,
        confidence_floor: Some(0.5),
        limit: 100,
    };

    let json = serde_json::to_value(&query).unwrap();

    assert_eq!(json["startIds"][0], "node-1");
    assert_eq!(json["edgeTypes"][1], "contains");
    assert_eq!(json["maxDepth"], 3);
    assert_eq!(json["confidenceFloor"], 0.5);
    assert_eq!(json["limit"], 100);
    assert!(json.get("start_ids").is_none());
    assert!(json.get("max_depth").is_none());
    assert!(json.get("confidence_floor").is_none());
}

#[test]
fn graph_query_dto_deserializes_with_defaults() {
    let json = serde_json::json!({
        "startIds": ["node-1"],
        "edgeTypes": []
    });

    let query: GraphQueryDto = serde_json::from_value(json).unwrap();

    assert_eq!(query.start_ids, vec!["node-1".to_string()]);
    assert!(query.edge_types.is_empty());
    assert_eq!(query.max_depth, 3);
    assert_eq!(query.confidence_floor, None);
    assert_eq!(query.limit, 100);
}

#[test]
fn graph_query_result_dto_serializes_camel_case() {
    let node = GraphNodeDto {
        id: "node-1".to_string(),
        case_id: "case-1".to_string(),
        node_type: GraphNodeTypeDto::Artifact,
        label: "LNK Artifact".to_string(),
        summary: "Shell link file".to_string(),
        tags: vec![],
        created_at: "2026-06-14T00:00:00Z".to_string(),
    };

    let edge = GraphEdgeDto {
        id: "edge-1".to_string(),
        case_id: "case-1".to_string(),
        source_id: "node-1".to_string(),
        target_id: "node-2".to_string(),
        edge_type: GraphEdgeTypeDto::References,
        confidence: Some(0.8),
        provenance: None,
        created_at: "2026-06-14T00:00:00Z".to_string(),
    };

    let result = GraphQueryResultDto {
        nodes: vec![node],
        edges: vec![edge],
        node_count: 1,
        edge_count: 1,
    };

    let json = serde_json::to_value(result).unwrap();

    assert_eq!(json["nodeCount"], 1);
    assert_eq!(json["edgeCount"], 1);
    assert_eq!(json["nodes"][0]["id"], "node-1");
    assert_eq!(json["edges"][0]["id"], "edge-1");
    assert!(json.get("node_count").is_none());
    assert!(json.get("edge_count").is_none());
}

#[test]
fn list_graph_nodes_request_defaults_and_camel_case() {
    let request: ListGraphNodesRequest =
        serde_json::from_value(serde_json::json!({})).expect("default request should deserialize");

    assert_eq!(request.limit, 100);
    assert_eq!(request.offset, 0);

    let json = serde_json::to_value(ListGraphNodesRequest {
        limit: 25,
        offset: 10,
    })
    .unwrap();

    assert_eq!(json["limit"], 25);
    assert_eq!(json["offset"], 10);
}

#[test]
fn graph_snapshot_dto_serializes_camel_case() {
    let mut node_count_by_type = std::collections::HashMap::new();
    node_count_by_type.insert("file".to_string(), 42);
    node_count_by_type.insert("artifact".to_string(), 15);

    let mut edge_count_by_type = std::collections::HashMap::new();
    edge_count_by_type.insert("references".to_string(), 30);
    edge_count_by_type.insert("contains".to_string(), 20);

    let snapshot = GraphSnapshotDto {
        node_count_by_type,
        edge_count_by_type,
        total_nodes: 57,
        total_edges: 50,
        density: 0.0313,
        largest_component_size: 40,
    };

    let json = serde_json::to_value(snapshot).unwrap();

    assert_eq!(json["totalNodes"], 57);
    assert_eq!(json["totalEdges"], 50);
    assert_eq!(json["density"], 0.0313);
    assert_eq!(json["largestComponentSize"], 40);
    assert_eq!(json["nodeCountByType"]["file"], 42);
    assert_eq!(json["nodeCountByType"]["artifact"], 15);
    assert_eq!(json["edgeCountByType"]["references"], 30);
    assert_eq!(json["edgeCountByType"]["contains"], 20);
    assert!(json.get("total_nodes").is_none());
    assert!(json.get("total_edges").is_none());
    assert!(json.get("node_count_by_type").is_none());
    assert!(json.get("edge_count_by_type").is_none());
    assert!(json.get("largest_component_size").is_none());
}
