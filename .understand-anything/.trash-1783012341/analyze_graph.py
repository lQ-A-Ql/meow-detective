import json
import sys
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

print(f'=== BASIC COUNTS ===')
print(f'Nodes: {len(nodes)}')
print(f'Edges: {len(edges)}')

node_types = Counter(n.get('type', 'NONE') for n in nodes)
print(f'\n=== NODE TYPES ===')
for t, c in node_types.most_common():
    print(f'  {t}: {c}')

edge_types = Counter(e.get('type', 'NONE') for e in edges)
print(f'\n=== EDGE TYPES ===')
for t, c in edge_types.most_common():
    print(f'  {t}: {c}')

complexities = Counter(n.get('complexity', 'NONE') for n in nodes)
print(f'\n=== COMPLEXITY VALUES ===')
for t, c in complexities.most_common():
    print(f'  {t}: {c}')

# Missing required fields
missing_id = sum(1 for n in nodes if not n.get('id'))
missing_type = sum(1 for n in nodes if not n.get('type'))
missing_name = sum(1 for n in nodes if not n.get('name'))
print(f'\n=== MISSING FIELDS ===')
print(f'  Missing id: {missing_id}')
print(f'  Missing type: {missing_type}')
print(f'  Missing name: {missing_name}')

# Unknown types
known_types = {'file', 'class', 'function', 'config', 'document', 'pipeline', 'step', 'table'}
unknown_types = Counter(n.get('type') for n in nodes if n.get('type') and n.get('type') not in known_types)
if unknown_types:
    print(f'\n=== UNKNOWN NODE TYPES ===')
    for t, c in unknown_types.most_common():
        samples = [n['id'] for n in nodes if n.get('type') == t][:5]
        print(f'  {t}: {c} (samples: {samples})')
else:
    print(f'\n=== NO UNKNOWN NODE TYPES ===')

# Unknown complexity
known_complexity = {'simple', 'moderate', 'complex'}
unknown_complexity = Counter(n.get('complexity') for n in nodes if n.get('complexity') and n.get('complexity') not in known_complexity)
if unknown_complexity:
    print(f'\n=== UNKNOWN COMPLEXITY VALUES ===')
    for t, c in unknown_complexity.most_common():
        samples = [n['id'] for n in nodes if n.get('complexity') == t][:5]
        print(f'  {t}: {c} (samples: {samples})')
else:
    print(f'\n=== NO UNKNOWN COMPLEXITY ===')

# Dangling edges
node_ids = {n['id'] for n in nodes if n.get('id')}
dangling = []
for e in edges:
    src_ok = e.get('source') in node_ids
    tgt_ok = e.get('target') in node_ids
    if not src_ok or not tgt_ok:
        dangling.append((e, 'source' if not src_ok else 'target'))
print(f'\n=== DANGLING EDGES ===')
print(f'Count: {len(dangling)}')
for e, problem in dangling[:30]:
    print(f'  {problem}={e.get(problem,"?")[:90]} type={e.get("type")}')

# ID prefix analysis
id_prefixes = Counter()
for n in nodes:
    nid = n.get('id', '')
    if ':' in nid:
        prefix = nid.split(':')[0]
        id_prefixes[prefix] += 1
print(f'\n=== ID PREFIXES ===')
for p, c in id_prefixes.most_common():
    print(f'  {p}: {c}')

# Import map cross-reference
print(f'\n=== IMPORT MAP CROSS-REFERENCE ===')
import_map_entries = len(import_map)
entries_with_imports = sum(1 for v in import_map.values() if len(v) > 0)
total_import_rels = sum(len(v) for v in import_map.values())
print(f'  Import map entries: {import_map_entries}')
print(f'  Entries with imports: {entries_with_imports}')
print(f'  Total import relationships: {total_import_rels}')

# Check which file nodes exist in the graph
file_node_ids = {n['id'] for n in nodes if n.get('type') == 'file'}
# Check edges of type 'imports' in the graph
import_edges = [e for e in edges if e.get('type') == 'imports']
import_edge_pairs = set()
for e in import_edges:
    import_edge_pairs.add((e['source'], e['target']))

print(f'  File nodes in graph: {len(file_node_ids)}')
print(f'  Import edges in graph: {len(import_edges)}')
print(f'  Unique import edge pairs: {len(import_edge_pairs)}')

# For each import map entry, check if the edge exists
missing_import_edges = []
for file_path, imports in import_map.items():
    if not imports:
        continue
    source_node_id = f'file:{file_path}'
    if source_node_id not in file_node_ids:
        continue  # source file not analyzed
    for imp in imports:
        target_node_id = f'file:{imp}'
        if target_node_id not in file_node_ids:
            continue  # target file not analyzed
        if (source_node_id, target_node_id) not in import_edge_pairs:
            missing_import_edges.append((source_node_id, target_node_id))

print(f'  Missing import edges (both nodes exist but no edge): {len(missing_import_edges)}')
for src, tgt in missing_import_edges[:30]:
    print(f'    {src} --> {tgt}')

# Duplicate edges check
edge_keys = {}
duplicate_edges = []
for e in edges:
    key = (e.get('source'), e.get('target'), e.get('type'))
    if key in edge_keys:
        duplicate_edges.append((key, edge_keys[key]))
    else:
        edge_keys[key] = e
print(f'\n=== DUPLICATE EDGES ===')
print(f'  Count: {len(duplicate_edges)}')
for (key, existing), _ in zip(duplicate_edges, duplicate_edges[:10]):
    print(f'  {key}')

# Orphan nodes (no edges at all)
nodes_with_edges = set()
for e in edges:
    nodes_with_edges.add(e.get('source'))
    nodes_with_edges.add(e.get('target'))
orphan_nodes = [n for n in nodes if n.get('id') not in nodes_with_edges]
print(f'\n=== ORPHAN NODES (no edges) ===')
print(f'  Count: {len(orphan_nodes)}')
orphan_by_type = Counter(n.get('type') for n in orphan_nodes)
for t, c in orphan_by_type.most_common():
    print(f'    {t}: {c}')

print(f'\n=== DONE ===')
