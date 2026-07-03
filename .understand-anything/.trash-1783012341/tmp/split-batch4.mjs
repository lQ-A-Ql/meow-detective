import { readFileSync, writeFileSync } from 'fs';

const graph = JSON.parse(readFileSync('D:/process/forensic/.understand-anything/tmp/batch-4-graph.json', 'utf8'));

const nodes = graph.nodes;
const edges = graph.edges;

console.log('Total nodes:', nodes.length, 'Total edges:', edges.length);

// Determine number of parts
const parts = Math.ceil(Math.max(nodes.length / 60, edges.length / 120));
console.log('Parts needed:', parts);

// Get the sorted list of unique file paths from nodes
const fileNodeMap = new Map(); // filePath -> node
const subFileNodeMap = new Map(); // nodeId -> filePath

for (const node of nodes) {
    if (node.filePath) {
        fileNodeMap.set(node.filePath, node);
    }
    subFileNodeMap.set(node.id, node.filePath);
}

// Sort files alphabetically by path
const sortedFilePaths = [...new Set(nodes.map(n => n.filePath).filter(Boolean))].sort();
console.log('Sorted files:', sortedFilePaths.length);

// Chunk files into parts
const filesPerPart = Math.ceil(sortedFilePaths.length / parts);
const fileChunks = [];
for (let i = 0; i < sortedFilePaths.length; i += filesPerPart) {
    fileChunks.push(sortedFilePaths.slice(i, i + filesPerPart));
}

console.log('Chunks:');
fileChunks.forEach((chunk, i) => console.log(`  Part ${i + 1}: ${chunk.length} files`));

// For each part, collect nodes and edges
for (let p = 0; p < fileChunks.length; p++) {
    const chunkFilePaths = new Set(fileChunks[p]);

    const partNodes = nodes.filter(n => {
        // File nodes: check filePath directly
        if (n.filePath && chunkFilePaths.has(n.filePath)) return true;
        // Sub-file nodes (function, class): check their filePath
        if (!n.filePath) return false;
        return chunkFilePaths.has(n.filePath);
    });

    const partNodeIds = new Set(partNodes.map(n => n.id));

    // Edges where source is in this part's nodes
    const partEdges = edges.filter(e => partNodeIds.has(e.source));

    // Validate: every edge's source must be in partNodeIds
    // (target can be anywhere)

    const outputPath = `D:/process/forensic/.understand-anything/intermediate/batch-4-part-${p + 1}.json`;
    writeFileSync(outputPath, JSON.stringify({ nodes: partNodes, edges: partEdges }, null, 2), 'utf8');
    console.log(`  Part ${p + 1}: ${partNodes.length} nodes, ${partEdges.length} edges -> ${outputPath}`);
}

console.log('\nDone splitting batch 4 into', parts, 'parts');
