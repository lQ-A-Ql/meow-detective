import json
from collections import Counter

graph_path = r'D:\process\forensic\.understand-anything\intermediate\assembled-graph.json'
scan_path = r'D:\process\forensic\.understand-anything\intermediate\scan-result.json'

with open(graph_path, 'r', encoding='utf-8') as f:
    graph = json.load(f)
with open(scan_path, 'r', encoding='utf-8') as f:
    scan = json.load(f)

nodes = graph.get('nodes', [])
edges = graph.get('edges', [])
import_map = scan.get('importMap', {})

file_node_ids = {n['id'] for n in nodes if n.get('type') == 'file'}
file_node_paths = {n['id']: n for n in nodes if n.get('type') == 'file'}
import_edges = [e for e in edges if e.get('type') == 'imports']
import_edge_pairs = set()
for e in import_edges:
    import_edge_pairs.add((e['source'], e['target']))

# Find missing edges
missing_import_edges = []
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
        if (source_node_id, target_node_id) not in import_edge_pairs:
            missing_import_edges.append((source_node_id, target_node_id, imp))

print(f"Total missing edges: {len(missing_import_edges)}")
print()

# Filter out self-referential
non_self = [(s, t, imp) for s, t, imp in missing_import_edges if s != t]
print(f"Non-self edges: {len(non_self)}")
for src, tgt, imp in non_self:
    print(f"  {src} --> {tgt}")

# Also check: does import_map actually have these imports?
print(f"\n=== Verifying against import map ===")
for src, tgt, imp in non_self:
    src_path = src.replace('file:', '')
    im_imports = import_map.get(src_path, [])
    if imp in im_imports:
        print(f"  VERIFIED: {src} imports {imp}")
    else:
        print(f"  NOT IN IMPORT MAP: {src} imports {imp} (import_map has: {im_imports[:3]}...)")

# Check orphan nodes in more detail
nodes_with_edges = set()
for e in edges:
    nodes_with_edges.add(e.get('source'))
    nodes_with_edges.add(e.get('target'))
orphan_nodes = [n for n in nodes if n.get('id') not in nodes_with_edges]
print(f"\n=== ORPHAN NODE DETAILS (first 20) ===")
for n in orphan_nodes[:20]:
    print(f"  {n['id']} (type={n.get('type')}, name={n.get('name')})")
