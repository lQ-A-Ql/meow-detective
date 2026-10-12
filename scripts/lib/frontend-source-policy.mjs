import path from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(new URL('../../frontend/package.json', import.meta.url));
const ts = require('typescript');

export function inspectFrontendSource(relativePath, text) {
  const violations = [];
  const source = ts.createSourceFile(relativePath, text, ts.ScriptTarget.Latest, true, relativePath.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const isFeatureView = /^frontend\/src\/features\/[^/]+\/components\//.test(relativePath);
  const isStyle = /^frontend\/src\/(styles|design)\//.test(relativePath);
  const isPrimitive = /^frontend\/src\/(app\/components\/ui|components)\//.test(relativePath);
  function report(node, message) {
    const { line } = source.getLineAndCharacterOfPosition(node.getStart(source));
    violations.push(`${relativePath}:${line + 1}: ${message}`);
  }
  if (relativePath.endsWith('.tsx') && text.trimEnd().split(/\r?\n/).length > 500) {
    violations.push(`${relativePath}: component files must stay within 500 lines`);
  }
  const importedRuntimeHooks = new Set();
  function visit(node) {
    if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier)) {
      const modulePath = node.moduleSpecifier.text;
      const clause = node.importClause;
      // UI-only translation/column factories and virtualizer libraries belong to rendering;
      // project state/IO hooks are resolved by path as well as imported symbol (including aliases).
      const resolvedPath = modulePath.startsWith('@/') ? `frontend/src/${modulePath.slice(2)}` : path.posix.normalize(path.posix.join(path.posix.dirname(relativePath), modulePath));
      const projectHook = /^@\/hooks\//.test(modulePath) || /(^|\/)(hooks|use-[^/]+)(\/|$)/.test(modulePath)
        || (/^frontend\/src\/features\//.test(resolvedPath) && !/^frontend\/src\/features\/[^/]+\/components\//.test(resolvedPath));
      if (clause && !clause.isTypeOnly && modulePath !== 'react' && modulePath !== 'react-i18next') {
        const bindings = clause.namedBindings;
        if (bindings && ts.isNamedImports(bindings)) {
          for (const item of bindings.elements) {
            if (item.isTypeOnly) continue;
            const importedName = item.propertyName?.text ?? item.name.text;
            if (projectHook && /^use[A-Z]/.test(importedName)) importedRuntimeHooks.add(item.name.text);
            if (isFeatureView && projectHook && (/^use[A-Z]/.test(importedName) || /^use[A-Z]/.test(item.name.text))) report(item, 'feature views may not execute runtime hooks; use a container');
          }
        }
        if (isFeatureView && (/^@\/hooks\//.test(modulePath) || /(^|\/)(hooks|use-[^/]+)(\/|$)/.test(modulePath) || /^@\/lib\/(api|platform)\//.test(modulePath) || /^@\/stores\//.test(modulePath))) {
          report(node, 'feature views may import model contracts only as types');
        }
        if (isFeatureView && bindings && ts.isNamespaceImport(bindings) && /features|(?:^|\/)hooks|(?:^|\/)use-/.test(modulePath)) report(node, 'namespace imports cannot bypass feature hook isolation');
      }
    }
    if (!isStyle && (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node))) {
      if (/#(?:[0-9a-f]{3,8})\b|(?:^|[^a-z0-9]|_)(?:rgb|hsl)a?\s*\(/i.test(node.text)) report(node, 'literal colors, including Tailwind shadows, belong in theme tokens');
    }
    if (isFeatureView && !isPrimitive && ts.isJsxAttribute(node) && node.name.getText(source) === 'className') {
      const content = node.initializer?.getText(source) ?? '';
      if (/\bshadow-(?:\[|workspace|context|sm|md|lg|xl)/.test(content) || (/\brounded-(?:ctl|bl-shell)/.test(content) && /\bborder(?:\s|$)/.test(content) && /\bbg-forensics-(?:paper|surface|panel-strong)\b/.test(content))) {
        report(node, 'card appearance must be owned by WorkspaceCard; inner sections stay flat');
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  // Hook aliases are tracked from actual imported symbols, not filename suffixes.
  if (isFeatureView && importedRuntimeHooks.size) {
    function calls(node) {
      if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && importedRuntimeHooks.has(node.expression.text)) report(node, 'runtime orchestration belongs in a feature container/model');
      ts.forEachChild(node, calls);
    }
    calls(source);
  }
  return [...new Set(violations)];
}

export function inspectSvgAsset(relativePath, text) {
  return /@keyframes\b|\banimation\s*:|<(?:animate|animateMotion|animateTransform)\b/.test(text)
    ? [`${relativePath}: motion must live in styles/motion, separate from SVG geometry`]
    : [];
}
