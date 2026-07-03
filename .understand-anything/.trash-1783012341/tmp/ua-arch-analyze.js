#!/usr/bin/env node
/**
 * Phase 1 -- Structural Analysis Script
 * Reads assembled-graph.json and produces structural metrics for layer identification.
 */
const fs = require('fs');
const path = require('path');

const inputPath = process.argv[2];
const outputPath = process.argv[3];

if (!inputPath || !outputPath) {
  console.error('Usage: node ua-arch-analyze.js <input-json> <output-json>');
  process.exit(1);
}

try {
  const rawData = JSON.parse(fs.readFileSync(inputPath, 'utf-8'));

  // Determine if the input is the assembled-graph (with nodes/edges) or the pre-filtered format (with fileNodes/importEdges/allEdges)
  let fileNodes, importEdges, allEdges;

  if (rawData.fileNodes) {
    // Pre-filtered format
    fileNodes = rawData.fileNodes;
    importEdges = rawData.importEdges;
    allEdges = rawData.allEdges;
  } else if (rawData.nodes && rawData.edges) {
    // Raw assembled-graph format - filter file-level
    const fileLevelTypes = new Set(['file', 'config', 'document', 'pipeline', 'table', 'schema', 'resource', 'service', 'endpoint']);
    fileNodes = rawData.nodes.filter(n => fileLevelTypes.has(n.type));
    const fileLevelNodeIds = new Set(fileNodes.map(n => n.id));
    importEdges = rawData.edges.filter(e => e.type === 'imports' && fileLevelNodeIds.has(e.source) && fileLevelNodeIds.has(e.target));
    allEdges = rawData.edges.filter(e => fileLevelNodeIds.has(e.source) && fileLevelNodeIds.has(e.target));
  } else {
    console.error('Unknown input format. Expected nodes/edges or fileNodes/importEdges/allEdges.');
    process.exit(1);
  }

  console.error(`Processing ${fileNodes.length} file-level nodes, ${importEdges.length} import edges, ${allEdges.length} all edges`);

  // ========== A. Directory Grouping ==========

  // Compute common path prefix
  const filePaths = fileNodes
    .filter(n => n.filePath)
    .map(n => n.filePath.replace(/\\/g, '/'));

  function commonPrefix(paths) {
    if (paths.length === 0) return '';
    const parts = paths[0].split('/');
    let prefixLen = 0;
    for (let i = 0; i < parts.length - 1; i++) {
      const candidate = parts.slice(0, i + 1).join('/') + '/';
      if (paths.every(p => p.startsWith(candidate) || p === candidate.slice(0, -1))) {
        prefixLen = i + 1;
      } else {
        break;
      }
    }
    return parts.slice(0, prefixLen).join('/') + (prefixLen > 0 ? '/' : '');
  }

  const commonPrefixStr = commonPrefix(filePaths);
  console.error(`Common path prefix: "${commonPrefixStr}"`);

  const directoryGroups = {};
  const allNodeIds = fileNodes.map(n => n.id);

  fileNodes.forEach(node => {
    const normalizedPath = (node.filePath || '').replace(/\\/g, '/');
    let groupKey;

    // Strip common prefix
    let relativePath = normalizedPath;
    if (commonPrefixStr && normalizedPath.startsWith(commonPrefixStr)) {
      relativePath = normalizedPath.slice(commonPrefixStr.length);
    }

    const parts = relativePath.split('/');
    if (parts.length >= 2 && parts[0] !== '') {
      groupKey = parts[0];
    } else if (parts.length === 1 && parts[0] !== '') {
      groupKey = '_root_';
    } else {
      // Try grouping by extension for flat structures
      const ext = path.extname(normalizedPath).toLowerCase();
      if (ext) {
        groupKey = `_ext_${ext.replace('.', '')}`;
      } else {
        groupKey = '_root_';
      }
    }

    if (!directoryGroups[groupKey]) {
      directoryGroups[groupKey] = [];
    }
    directoryGroups[groupKey].push(node.id);
  });

  console.error(`Directory groups: ${Object.keys(directoryGroups).length}`);
  Object.entries(directoryGroups).forEach(([k, v]) => console.error(`  ${k}: ${v.length} files`));

  // ========== B. Node Type Grouping ==========
  const nodeTypeGroups = {};
  fileNodes.forEach(node => {
    const t = node.type || 'unknown';
    if (!nodeTypeGroups[t]) nodeTypeGroups[t] = [];
    nodeTypeGroups[t].push(node.id);
  });

  // ========== C. Import Adjacency Matrix ==========
  const fileFanIn = {};
  const fileFanOut = {};

  // Initialize counts for all file-level nodes
  allNodeIds.forEach(id => {
    fileFanIn[id] = 0;
    fileFanOut[id] = 0;
  });

  importEdges.forEach(edge => {
    fileFanOut[edge.source] = (fileFanOut[edge.source] || 0) + 1;
    fileFanIn[edge.target] = (fileFanIn[edge.target] || 0) + 1;
  });

  // Group-level import tracking
  const nodeToGroup = {};
  Object.entries(directoryGroups).forEach(([group, ids]) => {
    ids.forEach(id => { nodeToGroup[id] = group; });
  });

  const groupImportFrom = {};  // group -> {otherGroup -> count}
  const groupImportedBy = {};  // group -> {otherGroup -> count}

  Object.keys(directoryGroups).forEach(g => {
    groupImportFrom[g] = {};
    groupImportedBy[g] = {};
  });

  importEdges.forEach(edge => {
    const fromGroup = nodeToGroup[edge.source];
    const toGroup = nodeToGroup[edge.target];
    if (fromGroup && toGroup && fromGroup !== toGroup) {
      groupImportFrom[fromGroup][toGroup] = (groupImportFrom[fromGroup][toGroup] || 0) + 1;
      groupImportedBy[toGroup][fromGroup] = (groupImportedBy[toGroup][fromGroup] || 0) + 1;
    }
  });

  // ========== D. Cross-Category Dependency Analysis ==========
  const crossCategoryEdges = [];
  const crossCatMap = {}; // "fromType->toType|edgeType" -> count

  allEdges.forEach(edge => {
    const fromNode = fileNodes.find(n => n.id === edge.source);
    const toNode = fileNodes.find(n => n.id === edge.target);
    if (fromNode && toNode && fromNode.type !== toNode.type) {
      const key = `${fromNode.type}->${toNode.type}|${edge.type}`;
      crossCatMap[key] = (crossCatMap[key] || 0) + 1;
    }
  });

  Object.entries(crossCatMap).forEach(([key, count]) => {
    const [types, edgeType] = key.split('|');
    const [fromType, toType] = types.split('->');
    crossCategoryEdges.push({ fromType, toType, edgeType, count });
  });

  // ========== E. Inter-Group Import Frequency ==========
  const interGroupImports = [];
  Object.entries(groupImportFrom).forEach(([from, targets]) => {
    Object.entries(targets).forEach(([to, count]) => {
      interGroupImports.push({ from, to, count });
    });
  });
  interGroupImports.sort((a, b) => b.count - a.count);

  // ========== F. Intra-Group Import Density ==========
  const intraGroupDensity = {};
  Object.entries(directoryGroups).forEach(([group, ids]) => {
    const idSet = new Set(ids);
    let internalEdges = 0;
    let totalEdges = 0;

    importEdges.forEach(edge => {
      if (idSet.has(edge.source) && idSet.has(edge.target)) {
        internalEdges++;
      }
      if (idSet.has(edge.source) || idSet.has(edge.target)) {
        totalEdges++;
      }
    });

    intraGroupDensity[group] = {
      internalEdges,
      totalEdges,
      density: totalEdges > 0 ? internalEdges / totalEdges : 0
    };
  });

  // ========== G. Directory Pattern Matching ==========

  const directoryPatternMap = {
    'routes': 'api', 'api': 'api', 'controllers': 'api', 'endpoints': 'api', 'handlers': 'api',
    'services': 'service', 'core': 'service', 'lib': 'service', 'domain': 'service', 'logic': 'service',
    'models': 'data', 'db': 'data', 'data': 'data', 'persistence': 'data', 'repository': 'data', 'entities': 'data',
    'components': 'ui', 'views': 'ui', 'pages': 'ui', 'ui': 'ui', 'layouts': 'ui', 'screens': 'ui',
    'middleware': 'middleware', 'plugins': 'middleware', 'interceptors': 'middleware', 'guards': 'middleware',
    'utils': 'utility', 'helpers': 'utility', 'common': 'utility', 'shared': 'utility', 'tools': 'utility',
    'config': 'config', 'constants': 'config', 'env': 'config', 'settings': 'config',
    '__tests__': 'test', 'test': 'test', 'tests': 'test', 'spec': 'test', 'specs': 'test',
    'types': 'types', 'interfaces': 'types', 'schemas': 'types', 'contracts': 'types', 'dtos': 'types',
    'hooks': 'hooks',
    'store': 'state', 'state': 'state', 'reducers': 'state', 'actions': 'state', 'slices': 'state',
    'assets': 'assets', 'static': 'assets', 'public': 'assets',
    'migrations': 'data',
    'management': 'config', 'commands': 'config',
    'templatetags': 'utility',
    'signals': 'service',
    'serializers': 'api',
    'cmd': 'entry',
    'internal': 'service',
    'pkg': 'utility',
    'dto': 'types', 'request': 'types', 'response': 'types',
    'entity': 'data',
    'controller': 'api',
    'routers': 'api',
    'composables': 'service',
    'blueprints': 'api',
    'mailers': 'service', 'jobs': 'service', 'channels': 'service',
    'bin': 'entry',
    'docs': 'documentation', 'documentation': 'documentation', 'wiki': 'documentation',
    'deploy': 'infrastructure', 'deployment': 'infrastructure', 'infra': 'infrastructure', 'infrastructure': 'infrastructure',
    '.github': 'ci-cd', '.gitlab': 'ci-cd', '.circleci': 'ci-cd',
    'k8s': 'infrastructure', 'kubernetes': 'infrastructure', 'helm': 'infrastructure', 'charts': 'infrastructure',
    'terraform': 'infrastructure', 'tf': 'infrastructure',
    'docker': 'infrastructure',
    'sql': 'data', 'database': 'data', 'schema': 'data',
    // Rust-specific patterns
    'src': 'service',
    'crates': 'service',
    // Frontend patterns
    'features': 'service',
    'stores': 'state',
    'lib': 'service',
    'app': 'ui',
    'i18n': 'config',
    'styles': 'assets',
  };

  // Special file-level patterns
  const testFilePatterns = [/\.(test|spec)\.(ts|tsx|js|jsx)$/, /^test_/, /_test\.(go|rs)$/, /Test\.(java|php)$/, /_spec\.rb$/, /Tests\.cs$/];
  const declarationFilePattern = /\.d\.ts$/;

  const patternMatches = {};

  Object.keys(directoryGroups).forEach(group => {
    const lowerGroup = group.toLowerCase().replace(/^_ext_/, '');

    // Try direct match first
    if (directoryPatternMap[lowerGroup]) {
      patternMatches[group] = directoryPatternMap[lowerGroup];
    } else if (lowerGroup.startsWith('.')) {
      patternMatches[group] = 'ci-cd';
    } else {
      // Check nested patterns
      const files = directoryGroups[group];
      const fileNodeMap = {};
      fileNodes.forEach(n => { fileNodeMap[n.id] = n; });

      // Determine by file content
      let bestPattern = null;
      const filePatternCounts = {};

      files.forEach(fileId => {
        const node = fileNodeMap[fileId];
        if (!node || !node.filePath) return;
        const fp = node.filePath.replace(/\\/g, '/');
        const fileName = path.basename(fp);

        // Check test patterns
        if (testFilePatterns.some(p => p.test(fileName))) {
          filePatternCounts['test'] = (filePatternCounts['test'] || 0) + 1;
        }
        // Check declaration files
        if (declarationFilePattern.test(fileName)) {
          filePatternCounts['types'] = (filePatternCounts['types'] || 0) + 1;
        }
      });

      if (Object.keys(filePatternCounts).length > 0) {
        bestPattern = Object.entries(filePatternCounts).sort((a, b) => b[1] - a[1])[0][0];
      }

      if (bestPattern) {
        patternMatches[group] = bestPattern;
      }
    }
  });

  // ========== H. Deployment Topology Detection ==========
  const deploymentTopology = {
    hasDockerfile: false,
    hasCompose: false,
    hasK8s: false,
    hasTerraform: false,
    hasCI: false,
    infraFiles: []
  };

  fileNodes.forEach(node => {
    const fp = (node.filePath || '').replace(/\\/g, '/');
    const fn = path.basename(fp);

    if (/^Dockerfile/i.test(fn)) {
      deploymentTopology.hasDockerfile = true;
      deploymentTopology.infraFiles.push(fp);
    }
    if (/^docker-compose/i.test(fn)) {
      deploymentTopology.hasCompose = true;
      deploymentTopology.infraFiles.push(fp);
    }
    if (/\.(tf|tfvars)$/i.test(fn)) {
      deploymentTopology.hasTerraform = true;
      deploymentTopology.infraFiles.push(fp);
    }
    if (fp.includes('.github/workflows/') || fp.includes('.gitlab-ci') || /Jenkinsfile/i.test(fn)) {
      deploymentTopology.hasCI = true;
      deploymentTopology.infraFiles.push(fp);
    }
    if (/k8s|kubernetes|helm/i.test(fp)) {
      deploymentTopology.hasK8s = true;
      deploymentTopology.infraFiles.push(fp);
    }
  });

  // ========== I. Data Pipeline Detection ==========
  const dataPipeline = {
    schemaFiles: [],
    migrationFiles: [],
    dataModelFiles: [],
    apiHandlerFiles: []
  };

  fileNodes.forEach(node => {
    const fp = (node.filePath || '').replace(/\\/g, '/');
    const fn = path.basename(fp);

    if (/\.sql$/i.test(fn) && /migration/i.test(fp)) {
      dataPipeline.migrationFiles.push(fp);
    } else if (/\.sql$/i.test(fn)) {
      dataPipeline.schemaFiles.push(fp);
    }
    if (/(model|entity|schema|repository)\b/i.test(fp) && !/\.sql$/i.test(fn)) {
      dataPipeline.dataModelFiles.push(fp);
    }
    if (/(route|handler|controller|api|endpoint|command)\b/i.test(fp) && !/\.sql$/i.test(fn)) {
      dataPipeline.apiHandlerFiles.push(fp);
    }
  });

  // ========== J. Documentation Coverage ==========
  const groupsWithDocs = new Set();
  const fileNodeMap = {};
  fileNodes.forEach(n => { fileNodeMap[n.id] = n; });

  const docFiles = fileNodes.filter(n => n.type === 'document');

  Object.entries(directoryGroups).forEach(([group, ids]) => {
    // Check if any file in this group has a README
    const hasReadme = ids.some(id => {
      const n = fileNodeMap[id];
      return n && n.filePath && /\/readme\.(md|rst)$/i.test(n.filePath.replace(/\\/g, '/'));
    });

    if (hasReadme) {
      groupsWithDocs.add(group);
    }

    // Check if any doc references this group
    docFiles.forEach(doc => {
      const edgesToGroup = allEdges.filter(e =>
        e.source === doc.id &&
        ids.includes(e.target)
      );
      if (edgesToGroup.length > 0) {
        groupsWithDocs.add(group);
      }
    });
  });

  const totalGroups = Object.keys(directoryGroups).length;
  const docCoverage = {
    groupsWithDocs: groupsWithDocs.size,
    totalGroups,
    coverageRatio: totalGroups > 0 ? groupsWithDocs.size / totalGroups : 0,
    undocumentedGroups: Object.keys(directoryGroups).filter(g => !groupsWithDocs.has(g))
  };

  // ========== K. Dependency Direction ==========
  const dependencyDirection = [];
  Object.entries(groupImportFrom).forEach(([from, targets]) => {
    Object.entries(targets).forEach(([to, count]) => {
      const reverseCount = (groupImportFrom[to] && groupImportFrom[to][from]) || 0;
      if (count > reverseCount) {
        dependencyDirection.push({
          dependent: from,
          dependsOn: to
        });
      }
    });
  });

  // ========== File Stats ==========
  const filesPerGroup = {};
  Object.entries(directoryGroups).forEach(([k, v]) => { filesPerGroup[k] = v.length; });

  const nodeTypeCounts = {};
  fileNodes.forEach(n => { nodeTypeCounts[n.type] = (nodeTypeCounts[n.type] || 0) + 1; });

  const fileStats = {
    totalFileNodes: fileNodes.length,
    filesPerGroup,
    nodeTypeCounts
  };

  // ========== Assemble Output ==========
  const output = {
    scriptCompleted: true,
    directoryGroups,
    nodeTypeGroups,
    crossCategoryEdges,
    interGroupImports,
    intraGroupDensity,
    patternMatches,
    deploymentTopology,
    dataPipeline,
    docCoverage,
    dependencyDirection,
    fileStats,
    fileFanIn,
    fileFanOut
  };

  fs.writeFileSync(outputPath, JSON.stringify(output, null, 2), 'utf-8');
  console.error('Analysis complete. Results written to', outputPath);
  process.exit(0);

} catch (err) {
  console.error('FATAL:', err.message);
  console.error(err.stack);
  process.exit(1);
}
