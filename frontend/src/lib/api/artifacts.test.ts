import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  getArtifactById,
  getArtifactFamilies,
  getArtifactFamilyCounts,
  getArtifactRows,
  getArtifactRowsPage,
} from './artifacts';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('artifacts API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getArtifactFamilies calls the correct command with no payload', async () => {
    requestMock.mockResolvedValueOnce(['browser', 'registry'] as never);
    const result = await getArtifactFamilies();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.artifacts.GET_ARTIFACT_FAMILIES);
    expect(result).toEqual(['browser', 'registry']);
  });

  it('getArtifactRows sends family filter when provided', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getArtifactRows('browser');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.artifacts.GET_ARTIFACT_ROWS, {
      family: 'browser',
    });
  });

  it('getArtifactRows sends undefined family when omitted', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getArtifactRows();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.artifacts.GET_ARTIFACT_ROWS, {
      family: undefined,
    });
  });

  it('getArtifactRowsPage forwards the opaque cursor without an offset', async () => {
    requestMock.mockResolvedValueOnce({ total: 1, items: [] } as never);
    await getArtifactRowsPage('browser', 'artifact-cursor-1', 200);
    expect(requestMock).toHaveBeenCalledWith(
      COMMANDS.artifacts.GET_ARTIFACT_ROWS_REQUEST,
      {
        request: {
          family: 'browser',
          limit: 200,
          cursor: 'artifact-cursor-1',
        },
      },
    );
  });

  it('getArtifactById sends artifactId in request payload', async () => {
    requestMock.mockResolvedValueOnce({ id: 'art-1' } as never);
    const result = await getArtifactById('art-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.artifacts.GET_ARTIFACT_BY_ID, {
      request: { artifactId: 'art-1' },
    });
    expect(result).toEqual({ id: 'art-1' });
  });

  it('getArtifactFamilyCounts calls the correct command', async () => {
    requestMock.mockResolvedValueOnce([
      { family: 'browser', count: 42 },
    ] as never);
    const result = await getArtifactFamilyCounts();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.artifacts.GET_ARTIFACT_FAMILY_COUNTS);
    expect(result).toEqual([{ family: 'browser', count: 42 }]);
  });
});
