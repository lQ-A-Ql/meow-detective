import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  getNodeNeighborhood,
  getGraphSnapshot,
  getProvenanceChain,
  queryGraph,
} from './graph';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('graph API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getGraphSnapshot calls the correct command with empty object', async () => {
    requestMock.mockResolvedValueOnce({ nodes: [], edges: [] } as never);
    const result = await getGraphSnapshot();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.graph.GET_GRAPH_SNAPSHOT, {});
    expect(result).toEqual({ nodes: [], edges: [] });
  });

  it('queryGraph sends the query object', async () => {
    const query = { nodeTypes: ['file'], edgeTypes: ['contains'] };
    requestMock.mockResolvedValueOnce({ nodes: [], edges: [] } as never);
    await queryGraph(query as never);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.graph.QUERY_GRAPH, { query });
  });

  it('getNodeNeighborhood sends nodeId and depth', async () => {
    requestMock.mockResolvedValueOnce({ nodes: [], edges: [] } as never);
    await getNodeNeighborhood('node-1', 3);
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.graph.GET_NODE_NEIGHBORHOOD, {
      nodeId: 'node-1',
      depth: 3,
    });
  });

  it('getNodeNeighborhood defaults depth to 1', async () => {
    requestMock.mockResolvedValueOnce({ nodes: [], edges: [] } as never);
    await getNodeNeighborhood('node-2');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.graph.GET_NODE_NEIGHBORHOOD, {
      nodeId: 'node-2',
      depth: 1,
    });
  });

  it('getProvenanceChain sends edgeId', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    const result = await getProvenanceChain('edge-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.graph.GET_PROVENANCE_CHAIN, {
      edgeId: 'edge-1',
    });
    expect(result).toEqual([]);
  });
});
