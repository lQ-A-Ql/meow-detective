import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { apiMode } from '@/lib/api/client';
import { EventEnvelope, EventTopic } from '@/types/models';
import { eventBus } from './bus';

export function subscribeToEvent<T = unknown>(topic: EventTopic, listener: (event: EventEnvelope<T>) => void) {
  if (apiMode === 'mock') {
    return eventBus.subscribeEvent(topic, listener);
  }

  let disposed = false;
  let unlisten: UnlistenFn | undefined;

  const pending = listen<EventEnvelope<T>>(topic, (event) => {
    listener(event.payload);
  }).then((callback) => {
    if (disposed) {
      callback();
      return;
    }

    unlisten = callback;
  });

  return () => {
    disposed = true;
    void pending.then(() => {
      unlisten?.();
    });
  };
}

export function publishMockEvent<T>(event: EventEnvelope<T>) {
  if (apiMode === 'mock') {
    eventBus.publishEvent(event);
  }
}
