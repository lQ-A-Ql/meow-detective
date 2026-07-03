import { readFileSync, writeFileSync } from 'fs';

const data = JSON.parse(readFileSync('D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-4.json', 'utf8'));

const results = data.results.map(r => ({
    path: r.path,
    language: r.language,
    fileCategory: r.fileCategory,
    totalLines: r.totalLines,
    nonEmptyLines: r.nonEmptyLines,
    functions: (r.functions || []).map(f => ({
        name: f.name,
        startLine: f.startLine,
        endLine: f.endLine,
        params: f.params,
        isExported: f.isExported
    })),
    classes: (r.classes || []).map(c => ({
        name: c.name,
        startLine: c.startLine,
        endLine: c.endLine,
        methods: c.methods,
        properties: c.properties,
        isExported: c.isExported
    })),
    exports: (r.exports || []).map(e => ({
        name: e.name,
        line: e.line,
        isDefault: e.isDefault
    })),
    metrics: r.metrics,
    callGraphCount: (r.callGraph || []).length,
    hasSections: !!r.sections,
    hasDefinitions: !!r.definitions,
    hasServices: !!r.services,
    hasEndpoints: !!r.endpoints,
    hasSteps: !!r.steps,
    hasResources: !!r.resources
}));

writeFileSync('D:/process/forensic/.understand-anything/tmp/ua-file-extract-results-4-summary.json', JSON.stringify(results, null, 2), 'utf8');
console.log('Done - wrote ' + results.length + ' results');
