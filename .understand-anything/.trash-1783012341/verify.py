import json
g = json.load(open(r'D:\process\forensic\.understand-anything\intermediate\assembled-graph.json', 'r', encoding='utf-8'))
print(f'Nodes: {len(g["nodes"])}, Edges: {len(g["edges"])}')
ie = [e for e in g['edges'] if e['type'] == 'imports']
print(f'Import edges: {len(ie)}')

# Quick re-check for dangling
nids = {n['id'] for n in g['nodes']}
d = [e for e in g['edges'] if e['source'] not in nids or e['target'] not in nids]
print(f'Dangling edges: {len(d)}')

# Check for duplicates
ek = {}
dups = 0
for e in g['edges']:
    key = (e['source'], e['target'], e['type'])
    if key in ek:
        dups += 1
    else:
        ek[key] = e
print(f'Duplicate edges: {dups}')
print('All checks passed!' if dups == 0 and len(d) == 0 else 'Issues found!')
