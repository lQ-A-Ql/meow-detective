import { EventEnvelope, EventTopic, JobSnapshot, TraceItem, WarningItem } from '@/types/models';

type EventMap = {
  jobs: JobSnapshot[];
  warnings: WarningItem[];
  trace: TraceItem[];
};

type Listener<K extends keyof EventMap> = (payload: EventMap[K]) => void;
type EventListener<T = unknown> = (event: EventEnvelope<T>) => void;

class EventBus {
  private listeners: { [K in keyof EventMap]?: Set<Listener<K>> } = {};
  private eventListeners: Partial<Record<EventTopic, Set<EventListener>>> = {};

  subscribe<K extends keyof EventMap>(topic: K, listener: Listener<K>) {
    let set = this.listeners[topic] as Set<Listener<K>> | undefined;
    if (!set) {
      set = new Set<Listener<K>>();
      this.listeners[topic] = set as { [P in keyof EventMap]?: Set<Listener<P>> }[K];
    }
    set.add(listener);
    return () => set.delete(listener);
  }

  publish<K extends keyof EventMap>(topic: K, payload: EventMap[K]) {
    const set = this.listeners[topic] as Set<Listener<K>> | undefined;
    set?.forEach((listener) => listener(payload));
  }

  subscribeEvent<T = unknown>(topic: EventTopic, listener: EventListener<T>) {
    const set = (this.eventListeners[topic] ??= new Set()) as Set<EventListener<T>>;
    set.add(listener);
    return () => set.delete(listener as EventListener);
  }

  publishEvent<T>(event: EventEnvelope<T>) {
    const set = this.eventListeners[event.topic] as Set<EventListener<T>> | undefined;
    set?.forEach((listener) => listener(event));
  }
}

export const eventBus = new EventBus();
