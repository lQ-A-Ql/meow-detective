type QueryRefetch = () => Promise<unknown>;

type AnalysisPlatform = 'windows' | 'linux' | undefined;

function queryError(result: unknown): unknown {
  if (!result || typeof result !== 'object' || !('error' in result)) {
    return undefined;
  }
  return result.error;
}

export async function refreshAnalysisQueries(
  platform: AnalysisPlatform,
  windowsQueries: readonly QueryRefetch[],
  linuxQuery?: QueryRefetch,
): Promise<void> {
  const results = platform === 'windows'
    ? await Promise.all(windowsQueries.map((query) => query()))
    : platform === 'linux' && linuxQuery
      ? [await linuxQuery()]
      : [];
  const error = results.map(queryError).find(Boolean);
  if (error) {
    throw error;
  }
}
