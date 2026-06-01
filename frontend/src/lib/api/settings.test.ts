import { describe, expect, it } from 'vitest';
import { getAppSettings, saveAppSettings } from './settings';

describe('settings API (mock mode)', () => {
  it('loads app settings without Tauri IPC in mock mode', async () => {
    const settings = await getAppSettings();

    expect(settings.caseRoot).toBeDefined();
    expect(settings.theme).toBe('light');
    expect(settings.imageSearchPaths).toEqual([]);
  });

  it('saves app settings through the API wrapper', async () => {
    const saved = await saveAppSettings({
      caseRoot: 'C:\\Cases',
      imageSearchPaths: ['D:\\Images'],
      theme: 'dark',
      devEventTrace: true,
    });

    expect(saved.theme).toBe('dark');
    expect(saved.imageSearchPaths).toEqual(['D:\\Images']);
  });
});
