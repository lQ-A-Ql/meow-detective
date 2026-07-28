import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { eventBus } from './bus';
import type { EventEnvelope, EventTopic } from '@/types/models';

let unlisteners: UnlistenFn[] = [];
let started = false;

/**
 * Start listening to Tauri backend events and bridge them to the frontend EventBus.
 * Safe to call multiple times (idempotent).
 */
export async function startTauriEventBridge(): Promise<void> {
  if (started) {
    return;
  }
  started = true;

  const topics: EventTopic[] = [
    'case-opened',
    'case-closed',
    'job-created',
    'job-started',
    'job-progress',
    'job-completed',
    'job-failed',
    'job-cancelled',
    'data-source-imported',
    'artifact-added',
    'timeline-updated',
    'search-index-progress',
    'partition-progress',
    'import-phase-progress',
    'import-partial-result',
    'job-cancellation',
    'cache-index-status',
    'performance-report-ready',
    'analysis-extraction-progress',
    'file-extract-progress',
  ];

  for (const topic of topics) {
    const unlisten = await listen<EventEnvelope>(topic, (event) => {
      eventBus.publishEvent(event.payload);
    });
    unlisteners.push(unlisten);
  }
}

/**
 * Stop all Tauri event listeners.
 */
export function stopTauriEventBridge(): void {
  unlisteners.forEach((unlisten) => unlisten());
  unlisteners = [];
  started = false;
}
