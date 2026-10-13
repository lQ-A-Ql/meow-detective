import { describe, expect, it } from 'vitest';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import fixture from '../../../mocks/registry-browser/data.json';
import wire from '../../../mocks/registry-browser/contract.json';
import type { RegistryHiveSession } from '@/types/registryBrowser';
import { registryFiletimeUtc, registryValueRows } from '@/features/registry-browser/value-model';
import i18n from '@/i18n';

describe('registry browser wire and synthetic adapter', () => {
  it('preserves the shared Rust session shape and decimal FILETIME', () => {
    const session: RegistryHiveSession = { ...wire.session, source: { ...wire.session.source, view: 'base' },
      hive: { ...wire.session.hive, readState: 'readable' }, rootKey: { ...wire.session.rootKey, readState: 'readable' } };
    expect(JSON.parse(JSON.stringify(session))).toEqual(wire.session);
    expect(session.rootKey.lastWriteFiletime).toBe('133700000000000001');
    expect(registryFiletimeUtc(session.rootKey.lastWriteFiletime)).toMatch(/Z$/);
    expect(registryFiletimeUtc('not-a-time')).toBeUndefined();
  });

  it('pages metadata without leaking raw payload or collapsing duplicate hive names', async () => {
    const client = createRegistryBrowserMock();
    const hives = await client.listHives({ dataSourceId: fixture.dataSourceId, limit: 100 });
    const users = hives.hives.filter((hive) => hive.name === 'NTUSER.DAT');
    expect(users).toHaveLength(2);
    expect(new Set(users.map((hive) => hive.fileId)).size).toBe(2);
    const session = await client.openHive({ fileId: hives.hives[0].fileId, view: 'base' });
    const key = await client.resolveKey({ sessionId: session.sessionId, keyPath: 'Microsoft\\Windows\\CurrentVersion' });
    const first = await client.listValues({ sessionId: session.sessionId, keyId: key.keyId, limit: 2 });
    const second = await client.listValues({ sessionId: session.sessionId, keyId: key.keyId, limit: 2, cursor: first.nextCursor });
    expect(first.values.map((value) => value.name)).toEqual(['', 'ProductName']);
    expect(second.values.map((value) => value.typeName)).toEqual(['REG_EXPAND_SZ', 'REG_QWORD']);
    expect(first.coverage.declaredCount).toBe(7);
    expect(first.values[0]).not.toHaveProperty('payload');
    expect(first.values[0]).not.toHaveProperty('keyId');
  });

  it('keeps large values visible and reads two exact bounded byte windows', async () => {
    const client = createRegistryBrowserMock();
    const session = await client.openHive({ fileId: fixture.hives[0].fileId, view: 'base' });
    const request = { sessionId: session.sessionId, keyId: 'current-version', valueId: 'large', length: 65536 };
    const first = await client.readValueRange({ ...request, offset: 0 });
    const second = await client.readValueRange({ ...request, offset: 65536 });
    expect(first.bytes).toHaveLength(65536);
    expect(second.bytes).toHaveLength(65536);
    expect(first.bytes.slice(0, 4)).toEqual([0, 1, 2, 3]);
    expect(first.eof).toBe(false);
    expect(second.eof).toBe(true);
    await expect(client.readValueRange({ ...request, length: 65537, offset: 0 })).rejects.toMatchObject({ code: 'invalidRequest' });
    await client.closeHive({ sessionId: session.sessionId });
    await expect(client.readValueRange({ ...request, offset: 0 })).rejects.toMatchObject({ code: 'expiredSession' });
  });

  it('distinguishes real default values, empty keys and partial unsupported results', async () => {
    const client = createRegistryBrowserMock();
    const session = await client.openHive({ fileId: fixture.hives[0].fileId, view: 'base' });
    const full = await client.listValues({ sessionId: session.sessionId, keyId: 'current-version', limit: 100 });
    const rows = registryValueRows(full.values, (key, options) => i18n.t(key, options));
    expect(rows[0].displayName).toBe('(默认)');
    expect(rows[0].displayPreview).toBe('空字符串');
    expect(rows.find((value) => value.typeCode === 11)?.preview).toBe('18446744073709551615');
    const empty = await client.listValues({ sessionId: session.sessionId, keyId: 'empty', limit: 100 });
    const partial = await client.listValues({ sessionId: session.sessionId, keyId: 'partial', limit: 100 });
    expect(empty.values).toEqual([]);
    expect(empty.coverage.declaredCount).toBe(0);
    expect(partial.values[0].typeCode).toBe(99);
    expect(partial.coverage).toMatchObject({ declaredCount: 2, returnedCount: 1, skippedCount: 1, status: 'partial' });
  });

  it('preserves UTF-16 code units including a supplementary-plane character', async () => {
    const client = createRegistryBrowserMock();
    const session = await client.openHive({ fileId: fixture.hives[0].fileId, view: 'base' });
    const range = await client.readValueRange({ sessionId: session.sessionId, keyId: 'current-version', valueId: 'unicode', offset: 0, length: 64 });
    expect(range.bytes).toEqual([0x3a, 0x79, 0x8b, 0x4f, 0x3d, 0xd8, 0x75, 0xdd, 0, 0]);
    expect(range.decodedText).toBe('示例🕵');
  });
});
