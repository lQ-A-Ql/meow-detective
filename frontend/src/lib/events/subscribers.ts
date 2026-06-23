import type { EventEnvelope, EventTopic } from '@/types/models';
import { eventBus } from './bus';

export function subscribeToEvent<T = unknown>(
  topic: EventTopic,
  listener: (event: EventEnvelope<T>) => void,
) {
  return eventBus.subscribeEvent(topic, listener);
}
