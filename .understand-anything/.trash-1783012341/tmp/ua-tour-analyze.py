#!/usr/bin/env python3
"""Phase 1: Graph Topology Analysis for Tour Builder."""
import json
import sys
from collections import defaultdict, deque

def load_graph(path):
    with open(path, 'r', encoding='utf-8') as f:
        return json.load(f)

def main(input_path, output_path):
    try:
        graph = load_graph(input_path)
    except Exception as e:
        print(f"FATAL: Failed to load input: {e}", file=sys.stderr)
        sys.exit(1)

    nodes = graph.get('nodes', [])
    edges = graph.get('edges', [])
    layers = graph.get('layers', [])

    # Index nodes by ID
    node_by_id = {}
    for n in nodes:
        node_by_id[n['id']] = n

    total_nodes = len(nodes)
    total_edges = len(edges)

    # --- H. Node Summary Index ---
    node_summary_index = {}
    for n in nodes:
        node_summary_index[n['id']] = {
            'name': n.get('name', ''),
            'type': n.get('type', 'unknown'),
            'summary': n.get('summary', '')
        }

    # --- A & B: Fan-In and Fan-Out ---
    fan_in = defaultdict(int)
    fan_out = defaultdict(int)

    for e in edges:
        src = e['source']
        tgt = e['target']
        fan_out[src] += 1
        fan_in[tgt] += 1

    # Rank by fan-in (descending)
    fan_in_ranking = sorted(
        [{'id': nid, 'fanIn': cnt, 'name': node_by_id.get(nid, {}).get('name', '')}
         for nid, cnt in fan_in.items()],
        key=lambda x: -x['fanIn']
    )[:50]

    # Rank by fan-out (descending)
    fan_out_ranking = sorted(
        [{'id': nid, 'fanOut': cnt, 'name': node_by_id.get(nid, {}).get('name', '')}
         for nid, cnt in fan_out.items()],
        key=lambda x: -x['fanOut']
    )[:50]

    # --- C: Entry Point Candidates ---
    entry_point_names = {
        'index.ts', 'index.tsx', 'index.js', 'index.jsx',
        'main.ts', 'main.tsx', 'main.js', 'main.jsx',
        'main.rs', 'main.go', 'main.py', 'main.c', 'main.cpp',
        'app.ts', 'app.tsx', 'app.js', 'app.jsx', 'app.py',
        'server.ts', 'server.js', 'mod.rs',
        'manage.py', 'wsgi.py', 'asgi.py', 'run.py', '__main__.py',
        'Application.java', 'Main.java', 'Program.cs',
        'config.ru', 'index.php', 'App.swift', 'Application.kt',
        'lib.rs'
    }

    file_nodes = [n for n in nodes if n['type'] == 'file']
    doc_nodes = [n for n in nodes if n['type'] == 'document']

    # Compute fan-out percentiles for file nodes
    file_fan_outs = sorted([fan_out.get(n['id'], 0) for n in file_nodes])
    n_files = len(file_fan_outs)
    if n_files > 0:
        top10_threshold = file_fan_outs[min(int(n_files * 0.9), n_files - 1)]
        bottom25_threshold = file_fan_outs[min(int(n_files * 0.25), n_files - 1)]
    else:
        top10_threshold = 1
        bottom25_threshold = 0

    entry_scores = []

    for n in file_nodes:
        score = 0
        name = n.get('name', '').lower()
        file_path = n.get('filePath', '')

        # Filename match
        if name in entry_point_names:
            score += 3

        # At project root or one level deep
        depth = file_path.count('/')
        if depth <= 1:
            score += 1

        # High fan-out (top 10%)
        fo = fan_out.get(n['id'], 0)
        if fo >= top10_threshold and top10_threshold > 0:
            score += 1

        # Low fan-in (bottom 25%)
        fi = fan_in.get(n['id'], 0)
        if fi <= bottom25_threshold:
            score += 1

        if score > 0:
            entry_scores.append({
                'id': n['id'],
                'score': score,
                'name': n.get('name', ''),
                'summary': n.get('summary', '')
            })

    for n in doc_nodes:
        score = 0
        file_path = n.get('filePath', '')
        name = n.get('name', '').lower()

        if name == 'readme.md' and (file_path == 'README.md' or file_path == 'readme.md'):
            score += 5
        elif name.endswith('.md') and file_path.count('/') == 0:
            score += 2

        if score > 0:
            entry_scores.append({
                'id': n['id'],
                'score': score,
                'name': n.get('name', ''),
                'summary': n.get('summary', '')
            })

    entry_scores.sort(key=lambda x: -x['score'])
    entry_point_candidates = entry_scores[:5]

    # --- D: BFS Traversal ---
    # Skip documentation nodes; find the top code entry point
    code_entry = None
    for ep in entry_point_candidates:
        if node_by_id.get(ep['id'], {}).get('type') != 'document':
            code_entry = ep['id']
            break

    if not code_entry and file_nodes:
        # Fallback: find any file with entry-point-like name
        for n in file_nodes:
            if n.get('name', '').lower() in entry_point_names:
                code_entry = n['id']
                break
        if not code_entry:
            code_entry = file_nodes[0]['id']

    bfs_order = []
    bfs_depth = {}
    bfs_by_depth = defaultdict(list)

    if code_entry:
        # Build adjacency: imports + calls edges, forward direction only
        adjacency = defaultdict(list)
        for e in edges:
            if e.get('type') in ('imports', 'calls'):
                adjacency[e['source']].append(e['target'])

        visited = set()
        queue = deque()
        queue.append((code_entry, 0))
        visited.add(code_entry)

        while queue:
            nid, depth = queue.popleft()
            bfs_order.append(nid)
            bfs_depth[nid] = depth
            bfs_by_depth[str(depth)].append(nid)

            for neighbor in adjacency.get(nid, []):
                if neighbor not in visited:
                    visited.add(neighbor)
                    queue.append((neighbor, depth + 1))
    else:
        bfs_order = []
        bfs_depth = {}
        bfs_by_depth = {}

    bfs_traversal = {
        'startNode': code_entry,
        'order': bfs_order,
        'depthMap': bfs_depth,
        'byDepth': {str(k): v for k, v in sorted(bfs_by_depth.items())}
    }

    # --- E. Non-Code File Inventory ---
    non_code = {
        'documentation': [],
        'infrastructure': [],
        'data': [],
        'config': []
    }

    for n in nodes:
        t = n.get('type', '')
        entry = {
            'id': n['id'],
            'name': n.get('name', ''),
            'summary': n.get('summary', '')
        }
        if t == 'document':
            non_code['documentation'].append(entry)
        elif t in ('service', 'pipeline', 'resource'):
            non_code['infrastructure'].append(entry)
        elif t in ('table', 'schema', 'endpoint'):
            non_code['data'].append(entry)
        elif t == 'config':
            non_code['config'].append(entry)

    # --- F. Tightly Coupled Clusters ---
    # Build bidirectional edge index for file-level nodes
    # We consider nodes that mutually import/call each other
    file_ids = set(n['id'] for n in file_nodes)

    # Build adjacency for files only
    file_adj = defaultdict(set)
    for e in edges:
        src, tgt, etype = e['source'], e['target'], e.get('type', '')
        if src in file_ids and tgt in file_ids and etype in ('imports', 'calls'):
            file_adj[src].add(tgt)

    # Find pairs with bidirectional relationship
    bidir_pairs = []
    for a in file_ids:
        for b in file_adj.get(a, set()):
            if a < b and a in file_adj.get(b, set()):
                bidir_pairs.append((a, b))

    # Build clusters from bidirectional pairs
    parent = {}

    def find(x):
        while parent.get(x, x) != x:
            parent[x] = parent.get(parent[x], parent[x])
            x = parent[x]
        return x

    def union(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb

    for a, b in bidir_pairs:
        if a not in parent:
            parent[a] = a
        if b not in parent:
            parent[b] = b
        union(a, b)

    clusters_map = defaultdict(list)
    for nid in parent:
        clusters_map[find(nid)].append(nid)

    # Expand clusters: add nodes connecting to 2+ existing members
    for root in list(clusters_map.keys()):
        members = set(clusters_map[root])
        for other in file_ids:
            if other in members:
                continue
            connections = sum(1 for m in members if other in file_adj.get(m, set()) or m in file_adj.get(other, set()))
            if connections >= 2:
                members.add(other)
                if other not in parent:
                    parent[other] = root
                else:
                    union(other, root)
        clusters_map[root] = list(members)

    # Filter to 2-5 nodes, rank by edge count among members
    compact_clusters = []
    seen_ids = set()
    for root, member_list in clusters_map.items():
        member_set = set(member_list)
        if 2 <= len(member_list) <= 5:
            # Count internal edges
            edge_count = 0
            for e in edges:
                if e['source'] in member_set and e['target'] in member_set:
                    edge_count += 1
            compact_clusters.append({
                'nodes': sorted(member_list),
                'edgeCount': edge_count
            })

    compact_clusters.sort(key=lambda x: -x['edgeCount'])
    clusters = compact_clusters[:10]

    # --- G. Layer List ---
    layer_list = []
    for l in layers:
        layer_list.append({
            'id': l.get('id', ''),
            'name': l.get('name', ''),
            'description': l.get('description', '')
        })

    # --- Output ---
    result = {
        'scriptCompleted': True,
        'entryPointCandidates': entry_point_candidates,
        'fanInRanking': fan_in_ranking[:20],
        'fanOutRanking': fan_out_ranking[:20],
        'bfsTraversal': bfs_traversal,
        'nonCodeFiles': non_code,
        'clusters': clusters,
        'layers': {
            'count': len(layers),
            'list': layer_list
        },
        'nodeSummaryIndex': node_summary_index,
        'totalNodes': total_nodes,
        'totalEdges': total_edges
    }

    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)

    print(f"Script completed successfully.")
    print(f"  Total nodes: {total_nodes}")
    print(f"  Total edges: {total_edges}")
    print(f"  Entry point candidates: {len(entry_point_candidates)}")
    print(f"  Fan-in ranking entries: {len(fan_in_ranking)}")
    print(f"  Fan-out ranking entries: {len(fan_out_ranking)}")
    print(f"  BFS traversal nodes reached: {len(bfs_order)}")
    print(f"  Non-code files: docs={len(non_code['documentation'])}, infra={len(non_code['infrastructure'])}, data={len(non_code['data'])}, config={len(non_code['config'])}")
    print(f"  Clusters: {len(clusters)}")
    print(f"  Layers: {len(layers)}")

    sys.exit(0)

if __name__ == '__main__':
    if len(sys.argv) != 3:
        print("Usage: ua-tour-analyze.py <input.json> <output.json>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])
