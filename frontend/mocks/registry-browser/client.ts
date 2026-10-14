import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryHiveDescriptor, RegistryKeyMetadata, RegistryValueMetadata } from '@/types/registryBrowser';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';
import fixture from './data.json';
import recoveryContract from './recovery-contract.json';
import type { RegistryRecovery, RegistryHiveView } from '@/types/registryBrowser';

export type RegistryPreviewScenario = 'standard' | 'emptySource' | 'failedSource';
type MockKey = RegistryKeyMetadata & { hive: string; children: string[] };
type Payload = { kind: string; text?: string; hex?: string };
type MockValue = RegistryValueMetadata & { keyId: string; payload: Payload };
const keys = fixture.keys as MockKey[];
const values = fixture.values as MockValue[];
const hives = fixture.hives as RegistryHiveDescriptor[];
const roots = fixture.roots as Record<string, string>;

function failure(code: string) { return Object.assign(new Error(code), { code }); }
function metadata<T extends MockKey | MockValue>(item: T) {
  const { hive: _hive, children: _children, keyId: _keyId, payload: _payload, ...rest } = item as MockKey & MockValue;
  return 'payload' in item ? rest as unknown as RegistryValueMetadata : { ...rest, keyId: item.keyId } as RegistryKeyMetadata;
}
function page<T>(items: T[], cursor: string | undefined, limit: number, declared = items.length) {
  const offset = cursor ? Number(cursor) : 0;
  if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isInteger(limit) || limit < 1 || limit > 100) throw failure('invalidRequest');
  const rows = items.slice(offset, offset + limit);
  return { rows, nextCursor: offset + rows.length < items.length ? String(offset + rows.length) : undefined,
    coverage: { declaredCount: declared, returnedCount: rows.length, skippedCount: declared - items.length,
      status: declared === items.length ? 'complete' as const : 'partial' as const } };
}
function payloadRange(value: MockValue, offset: number, length: number) {
  const end = Math.min(value.byteLength, offset + length);
  if (value.payload.kind === 'pattern') return Array.from({ length: Math.max(0, end - offset) }, (_, index) => (offset + index) % 256);
  const bytes = value.payload.kind === 'utf16'
    ? ((value.payload.text ?? '') + '\0').split('').flatMap((character) => { const code = character.charCodeAt(0); return [code & 255, code >> 8]; })
    : (value.payload.hex ?? '').match(/../g)?.map((pair) => parseInt(pair, 16)) ?? [];
  if (bytes.length !== value.byteLength) throw failure('invalidFixture');
  return bytes.slice(offset, end);
}

export function createRegistryBrowserMock(scenario: RegistryPreviewScenario = 'standard', delayMs = 0): RegistryBrowserClient {
  const sessions = new Map<string, { fileId: string; hive: string; view: RegistryHiveView }>();
  let sequence = 0;
  const pause = () => new Promise<void>((resolve) => setTimeout(resolve, delayMs));
  const owned = (sessionId: string, keyId: string) => {
    const session = sessions.get(sessionId);
    const key = keys.find((item) => item.keyId === keyId && item.hive === session?.hive);
    if (!session || !key) throw failure('expiredSession');
    if (key.readState === 'corrupt') throw failure('corruptKey');
    return key;
  };
  return {
    async listHives(request) {
      await pause();
      if (scenario === 'failedSource') throw failure('sourceUnavailable');
      const result = page(scenario === 'emptySource' ? [] : hives, request.cursor, request.limit);
      return { sourceRevision: 'synthetic-v1', hives: result.rows, coverage: result.coverage, nextCursor: result.nextCursor, diagnostics: [] };
    },
    async openHive(request) {
      await pause();
      const hive = hives.find((item) => item.fileId === request.fileId);
      if (!hive || !['base', 'recovered'].includes(request.view)) throw failure('invalidRequest');
      if (hive.readState === 'locked') throw failure('lockedHive');
      const name = hive.fileId.split(':').slice(-1)[0];
      const root = keys.find((key) => key.keyId === roots[name])!;
      const sessionId = `synthetic-session-${++sequence}`;
      sessions.set(sessionId, { fileId: hive.fileId, hive: name, view: request.view });
      return { sessionId, source: { caseId: fixture.caseId, dataSourceId: hive.dataSourceId, fileId: hive.fileId,
        sourceRevision: 'synthetic-v1', view: request.view }, hive, rootKey: metadata(root) as RegistryKeyMetadata,
        expiresAt: '2099-01-01T00:00:00Z', diagnostics: hive.name === 'SOFTWARE' && request.view === 'base'
          ? [{ code: 'transactionLogsNotMerged', recoverable: false, itemId: hive.fileId }] : [],
        recovery: request.view === 'recovered' ? { ...recoveryContract, status: hive.name === 'SOFTWARE' ? 'recovered' : 'notNeeded' } as RegistryRecovery : undefined };
    },
    async resolveKey(request) {
      await pause();
      const session = sessions.get(request.sessionId);
      const key = keys.find((item) => item.hive === session?.hive && item.path.toLowerCase() === request.keyPath.toLowerCase());
      if (!key) throw failure('keyNotFound');
      return metadata(owned(request.sessionId, key.keyId)) as RegistryKeyMetadata;
    },
    async listSubkeys(request) {
      await pause();
      const parent = owned(request.sessionId, request.keyId);
      const children = parent.children.map((id) => metadata(keys.find((item) => item.keyId === id)!) as RegistryKeyMetadata);
      const result = page(children, request.cursor, request.limit);
      return { keys: result.rows, coverage: result.coverage, nextCursor: result.nextCursor, diagnostics: [] };
    },
    async listValues(request) {
      await pause();
      const parent = owned(request.sessionId, request.keyId);
      const rows = values.filter((item) => item.keyId === parent.keyId).map((item) => metadata(item) as RegistryValueMetadata);
      const result = page(rows, request.cursor, request.limit, parent.valueCount);
      return { values: result.rows, coverage: result.coverage, nextCursor: result.nextCursor,
        diagnostics: result.coverage.status === 'partial' ? [{ code: 'partialValues', recoverable: false }] : [] };
    },
    async readValueRange(request) {
      await pause(); owned(request.sessionId, request.keyId);
      const value = values.find((item) => item.keyId === request.keyId && item.valueId === request.valueId);
      if (!value || !Number.isSafeInteger(request.offset) || request.offset < 0 || !Number.isInteger(request.length)
        || request.length < 1 || request.length > REGISTRY_BROWSER_LAYOUT.valueRangeBytes) throw failure('invalidRequest');
      const bytes = payloadRange(value, request.offset, request.length);
      return { valueId: value.valueId, offset: request.offset, totalLength: value.byteLength, bytes,
        eof: request.offset + bytes.length >= value.byteLength,
        decodeStatus: value.payload.text !== undefined ? 'decoded' : 'rawOnly', decodedText: value.payload.text,
        diagnostics: [] };
    },
    async searchHive() { return { hits: [], scannedKeys: 0, scannedValues: 0, skippedCount: 0, truncated: false }; },
    async closeHive(request) { sessions.delete(request.sessionId); return true; },
  };
}
