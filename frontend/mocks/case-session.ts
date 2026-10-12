import fixture from './case-home.json';

let current = new URLSearchParams(window.location.search).get('case') === 'none' ? null : fixture.currentCase;

export function currentCase() {
  return current;
}

export function createCase(request: Record<string, unknown>) {
  current = { ...fixture.currentCase, id: 'case-created-preview', name: String(request.name ?? fixture.currentCase.name) };
  return current;
}

export function openCase(request: Record<string, unknown>) {
  const recent = fixture.recentCases.find((item) => item.caseRoot === request.caseRoot);
  current = { ...fixture.currentCase, name: recent?.name ?? fixture.currentCase.name };
  return current;
}

export function closeCase() {
  current = null;
}
