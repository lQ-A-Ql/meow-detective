import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ErrorBoundary } from '@/app/components/ErrorBoundary';

function ThrowingComponent(): React.ReactNode {
  throw new Error('Test error');
}

function SafeComponent() {
  return <div>All good</div>;
}

function suppressExpectedRenderError(): () => void {
  const listener = (event: ErrorEvent) => {
    if (event.error instanceof Error && event.error.message === 'Test error') {
      event.preventDefault();
    }
  };
  window.addEventListener('error', listener);
  return () => window.removeEventListener('error', listener);
}

describe('ErrorBoundary', () => {
  it('renders children when no error', () => {
    render(
      <ErrorBoundary>
        <SafeComponent />
      </ErrorBoundary>,
    );
    expect(screen.getByText('All good')).toBeDefined();
  });

  it('renders error UI when child throws', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const restoreWindowError = suppressExpectedRenderError();

    try {
      render(
        <ErrorBoundary>
          <ThrowingComponent />
        </ErrorBoundary>,
      );

      expect(screen.getByText('应用发生错误')).toBeDefined();
      expect(screen.getByText('Test error')).toBeDefined();
      expect(screen.getByText('重新加载')).toBeDefined();
      expect(screen.getByText('尝试恢复')).toBeDefined();
    } finally {
      restoreWindowError();
      consoleSpy.mockRestore();
    }
  });

  it('renders custom fallback when provided', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const restoreWindowError = suppressExpectedRenderError();

    try {
      render(
        <ErrorBoundary fallback={<div>Custom fallback</div>}>
          <ThrowingComponent />
        </ErrorBoundary>,
      );

      expect(screen.getByText('Custom fallback')).toBeDefined();
    } finally {
      restoreWindowError();
      consoleSpy.mockRestore();
    }
  });
});
