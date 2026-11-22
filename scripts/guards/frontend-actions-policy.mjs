import { createRequire } from 'node:module';

const requireFrontend = createRequire(new URL('../../frontend/package.json', import.meta.url));
const ts = requireFrontend('typescript');
const labeledShapes = new Set(['pill', 'row', 'card', 'bar', 'tab']);
const exemptFiles = new Set([
  'frontend/src/components/controls/IconButton.tsx',
  'frontend/src/components/navigation/ActivityBar.tsx',
]);

function labeledShapeCondition(node, shapeExpression) {
  if (!shapeExpression || !ts.isBinaryExpression(node)
    || ![ts.SyntaxKind.EqualsEqualsToken, ts.SyntaxKind.EqualsEqualsEqualsToken].includes(node.operatorToken.kind)) return false;
  return [[node.left, node.right], [node.right, node.left]].some(([reference, value]) =>
    reference.getText() === shapeExpression && ts.isStringLiteral(value) && labeledShapes.has(value.text));
}

function rendersText(node, shapeExpression) {
  if (ts.isJsxText(node)) return node.text.trim().length > 0;
  if (ts.isJsxExpression(node)) return node.expression ? rendersText(node.expression, shapeExpression) : false;
  if (ts.isJsxSelfClosingElement(node)) return false;
  if (ts.isJsxElement(node)) {
    return node.openingElement.tagName.getText() !== 'Badge' && node.children.some((child) => rendersText(child, shapeExpression));
  }
  if (ts.isJsxFragment(node)) return node.children.some((child) => rendersText(child, shapeExpression));
  if (ts.isConditionalExpression(node)) {
    return (!labeledShapeCondition(node.condition, shapeExpression) && rendersText(node.whenTrue, shapeExpression))
      || rendersText(node.whenFalse, shapeExpression);
  }
  if (ts.isBinaryExpression(node)) return rendersText(node.right, shapeExpression);
  if (ts.isParenthesizedExpression(node)) return rendersText(node.expression, shapeExpression);
  return ts.isIdentifier(node) || ts.isPropertyAccessExpression(node) || ts.isStringLiteral(node)
    || ts.isNumericLiteral(node) || ts.isTemplateExpression(node) || ts.isNoSubstitutionTemplateLiteral(node)
    || (ts.isCallExpression(node) && node.expression.getText() === 't');
}

function hasLabeledShape(opening) {
  return opening.attributes.properties.some((attribute) =>
    ts.isJsxAttribute(attribute) && attribute.name.getText() === 'shape'
      && attribute.initializer && ts.isStringLiteral(attribute.initializer)
      && labeledShapes.has(attribute.initializer.text));
}

function dynamicShape(opening) {
  const attribute = opening.attributes.properties.find((candidate) =>
    ts.isJsxAttribute(candidate) && candidate.name.getText() === 'shape');
  return attribute?.initializer && ts.isJsxExpression(attribute.initializer)
    ? attribute.initializer.expression?.getText() : undefined;
}

export function actionButtonViolations(content, filename) {
  if (exemptFiles.has(filename)) return [];
  const source = ts.createSourceFile(filename, content, ts.ScriptTarget.Latest, true,
    filename.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  if (source.parseDiagnostics.length > 0) {
    return source.parseDiagnostics.map((diagnostic) => ({
      line: source.getLineAndCharacterOfPosition(diagnostic.start ?? 0).line + 1,
      message: ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n'),
    }));
  }
  const buttonNames = new Set(['Button']);
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement) || !ts.isStringLiteral(statement.moduleSpecifier)
      || statement.moduleSpecifier.text !== '@/app/components/ui/button') continue;
    const bindings = statement.importClause?.namedBindings;
    if (!bindings || !ts.isNamedImports(bindings)) continue;
    for (const specifier of bindings.elements) {
      if ((specifier.propertyName ?? specifier.name).text === 'Button') buttonNames.add(specifier.name.text);
    }
  }
  const violations = [];
  function visit(node) {
    if (ts.isJsxElement(node) && buttonNames.has(node.openingElement.tagName.getText())
      && !hasLabeledShape(node.openingElement)
      && node.children.some((child) => rendersText(child, dynamicShape(node.openingElement)))) {
      violations.push({
        line: source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1,
        message: 'circular actions require icon-only children; labeled actions require an explicit labeled shape',
      });
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return violations;
}
