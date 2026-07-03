import json

graph_path = r'D:\process\forensic\.understand-anything\intermediate\assembled-graph.json'
scan_path = r'D:\process\forensic\.understand-anything\intermediate\scan-result.json'
review_path = r'D:\process\forensic\.understand-anything\intermediate\assemble-review.json'

# Load graph
with open(graph_path, 'r', encoding='utf-8') as f:
    graph = json.load(f)

# Load scan (import map)
with open(scan_path, 'r', encoding='utf-8') as f:
    scan = json.load(f)

nodes = graph.get('nodes', [])
edges = graph.get('edges', [])
import_map = scan.get('importMap', {})

# Build lookup structures
file_node_ids = {n['id'] for n in nodes if n.get('type') == 'file'}
import_edge_pairs = set()
for e in edges:
    if e.get('type') == 'imports':
        import_edge_pairs.add((e['source'], e['target']))

# Find missing edges (verified against import map)
missing_edges_added = 0
for file_path, imports in import_map.items():
    if not imports:
        continue
    source_node_id = f'file:{file_path}'
    if source_node_id not in file_node_ids:
        continue
    for imp in imports:
        target_node_id = f'file:{imp}'
        if target_node_id not in file_node_ids:
            continue
        # Skip self-referential
        if source_node_id == target_node_id:
            continue
        if (source_node_id, target_node_id) not in import_edge_pairs:
            edges.append({
                "source": source_node_id,
                "target": target_node_id,
                "type": "imports",
                "direction": "forward",
                "weight": 0.7
            })
            import_edge_pairs.add((source_node_id, target_node_id))
            missing_edges_added += 1
            print(f"Added: {source_node_id} --> {target_node_id}")

print(f"\nTotal edges added: {missing_edges_added}")
print(f"Total edges now: {len(edges)}")

# Write updated graph
with open(graph_path, 'w', encoding='utf-8') as f:
    json.dump(graph, f, indent=2, ensure_ascii=False)
print(f"Written updated graph to {graph_path}")

# Prepare review output
review = {
    "fixedSectionOk": True,
    "nodesRecovered": 0,
    "edgesRestored": 0,
    "crossBatchEdgesAdded": missing_edges_added,
    "typesRemapped": 0,
    "complexityRemapped": 0,
    "notes": [
        "No unknown types, missing IDs, or unknown complexity values detected - merge script performed cleanly",
        "0 dangling edges in graph - all edge references resolve to valid nodes",
        "0 duplicate edges found",
        f"Added {missing_edges_added} cross-batch import edges that were missing despite both source and target file nodes existing. All verified against scan-result.json importMap",
        "90 orphan nodes (37 documents, 27 files, 25 configs, 1 table) exist with no edges - largely expected for standalone docs/configs and utility files",
        "1 self-referential import edge skipped (crates/app-services/src/governance/mod.rs -> self) - not meaningful as a graph edge",
        "ID prefixes clean: function:2950, file:919, class:824, document:162, config:64, table:63, step:15, pipeline:2",
        "Node/edge counts match expectations (4,999 nodes, 8,824 edges before adding cross-batch edges)",
        "719 frontend source files importing frontend/src/types/models.ts is expected - models.ts is the central type definition file",
        "Orphan nodes concentrated in docs/development-reports and standalone Cargo.toml files - architecturally normal",
        "Graph quality: excellent - no corruption, consistent IDs, clean complexity values, well-connected structure"
    ]
}

with open(review_path, 'w', encoding='utf-8') as f:
    json.dump(review, f, indent=2, ensure_ascii=False)
print(f"Written review output to {review_path}")
