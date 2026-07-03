import fs from 'fs';

const data = JSON.parse(fs.readFileSync('D:/process/forensic/.understand-anything/intermediate/batch-2.json','utf8'));

// Get all file-level node paths
const fileNodes = data.nodes.filter(n => n.filePath);
const filePaths = [...new Set(fileNodes.map(n => n.filePath))].sort();

// 4 parts
const parts = 4;
const chunkSize = Math.ceil(filePaths.length / parts);
console.log('Total files: ' + filePaths.length + ', parts: ' + parts + ', chunk size: ' + chunkSize);

const filePathToPart = {};
for (let i = 0; i < filePaths.length; i++) {
  filePathToPart[filePaths[i]] = Math.floor(i / chunkSize) + 1;
}

// Determine which part each node belongs to
// File nodes: by filePath
// Non-file nodes: by filePath field
const nodeToPart = {};
for (const node of data.nodes) {
  const fp = node.filePath || '';
  if (fp && filePathToPart[fp]) {
    nodeToPart[node.id] = filePathToPart[fp];
  } else {
    // Try to infer from ID
    const match = node.id.match(/^(function|class):(.+?):/);
    if (match && filePathToPart[match[2]]) {
      nodeToPart[node.id] = filePathToPart[match[2]];
    } else {
      // Fallback to part 1
      nodeToPart[node.id] = 1;
    }
  }
}

for (let p = 1; p <= parts; p++) {
  const partNodes = data.nodes.filter(n => nodeToPart[n.id] === p);
  const partNodeIds = new Set(partNodes.map(n => n.id));
  const partEdges = data.edges.filter(e => partNodeIds.has(e.source));

  // Verify targets exist or are in neighborMap/importData
  // Skip strict validation for now - merge script handles dangling edges

  const part = { nodes: partNodes, edges: partEdges };
  fs.writeFileSync('D:/process/forensic/.understand-anything/intermediate/batch-2-part-' + p + '.json', JSON.stringify(part, null, 2));
  console.log('Part ' + p + ': ' + partNodes.length + ' nodes, ' + partEdges.length + ' edges');
}

// Delete the single-part file
fs.unlinkSync('D:/process/forensic/.understand-anything/intermediate/batch-2.json');

// Verify totals
let totalNodes = 0, totalEdges = 0;
for (let p = 1; p <= parts; p++) {
  const part = JSON.parse(fs.readFileSync('D:/process/forensic/.understand-anything/intermediate/batch-2-part-' + p + '.json','utf8'));
  totalNodes += part.nodes.length;
  totalEdges += part.edges.length;
}
console.log('Total across parts: ' + totalNodes + ' nodes, ' + totalEdges + ' edges');
console.log('Done');
