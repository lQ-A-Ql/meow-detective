import { apiClient } from './client';

export async function getTimelineEvents() {
  return apiClient.request('get_timeline_events', () => apiClient.getMockProvider().getTimelineEvents());
}
