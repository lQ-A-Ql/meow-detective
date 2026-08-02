import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { DenseDataTable, type DenseColumn } from './DenseDataTable';

interface Row {
  id: string;
  name: string;
}

const columns: DenseColumn<Row>[] = [
  {
    key: 'name',
    title: 'Name',
    sortable: true,
    render: (row) => row.name,
  },
];

beforeEach(() => {
  vi.stubGlobal('ResizeObserver', class {
    observe = vi.fn();
    disconnect = vi.fn();
    unobserve = vi.fn();
  });
});

function getScrollViewport(container: HTMLElement) {
  const viewport = container.querySelector('[data-slot="scroll-area-viewport"]');
  if (!(viewport instanceof HTMLDivElement)) {
    throw new Error('DenseDataTable scroll viewport was not rendered.');
  }
  return viewport;
}

describe('DenseDataTable', () => {
  it('renders only a bounded DOM window for large datasets', () => {
    const rows = Array.from({ length: 10_000 }, (_, index) => ({
      id: `row-${index}`,
      name: `Row ${index}`,
    }));

    const { container } = render(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
      />
    );

    const bodyRows = container.querySelectorAll('tbody tr');
    expect(bodyRows.length).toBeLessThan(100);
    expect(container.textContent).toContain('Row 0');
    expect(container.textContent).not.toContain('Row 9999');
  });

  it('updates the visible window as the table scrolls', () => {
    const rows = Array.from({ length: 10_000 }, (_, index) => ({
      id: `row-${index}`,
      name: `Row ${index}`,
    }));

    const { container } = render(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
      />
    );

    const scrollContainer = getScrollViewport(container);

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 310_000 } });

    const bodyRows = container.querySelectorAll('tbody tr');
    expect(bodyRows.length).toBeLessThan(100);
    expect(container.textContent).toContain('Row 9999');
    expect(container.textContent).not.toContain('Row 0');
  });

  it('preserves external sort and filter interactions', () => {
    function Harness() {
      const [ascending, setAscending] = React.useState(true);
      const [filtered, setFiltered] = React.useState(false);

      const rows = React.useMemo(() => {
        const source = [
          { id: 'row-a', name: 'Alpha' },
          { id: 'row-b', name: 'Bravo' },
          { id: 'row-c', name: 'Charlie' },
        ];

        const sorted = ascending ? source : [...source].reverse();
        return filtered ? sorted.filter((row) => row.name !== 'Bravo') : sorted;
      }, [ascending, filtered]);

      return (
        <div>
          <button type="button" onClick={() => setFiltered(true)}>
            Filter rows
          </button>
          <DenseDataTable
            columns={columns}
            rows={rows}
            getRowKey={(row) => row.id}
            sortKey="name"
            sortDirection={ascending ? 'asc' : 'desc'}
            onSort={() => setAscending((current) => !current)}
          />
        </div>
      );
    }

    render(<Harness />);

    const header = screen.getByText('Name');
    fireEvent.click(header);

    const renderedRowsAfterSort = screen.getAllByRole('row').slice(1);
    expect(renderedRowsAfterSort[0]?.textContent).toContain('Charlie');
    expect(renderedRowsAfterSort[1]?.textContent).toContain('Bravo');
    expect(renderedRowsAfterSort[2]?.textContent).toContain('Alpha');

    fireEvent.click(screen.getByText('Filter rows'));

    expect(screen.queryByText('Bravo')).toBeNull();
    const renderedRowsAfterFilter = screen.getAllByRole('row').slice(1);
    expect(renderedRowsAfterFilter).toHaveLength(2);
    expect(renderedRowsAfterFilter[0]?.textContent).toContain('Charlie');
    expect(renderedRowsAfterFilter[1]?.textContent).toContain('Alpha');
  });

  it('uses fixed columns with independent hover-revealed cell scroll regions', () => {
    const { container } = render(
      <DenseDataTable
        columns={columns}
        rows={[{ id: 'row-1', name: 'A long value that remains selectable inside its own cell' }]}
        getRowKey={(row) => row.id}
      />,
    );

    const table = container.querySelector('[data-slot="table"]');
    expect(table?.className).toContain('table-fixed');

    const cellScroller = container.querySelector('.scrollbar-thin-glow--cell');
    expect(cellScroller).not.toBeNull();
    expect(cellScroller?.className).toContain('scrollbar-thin-glow--reveal-on-hover');
    expect(cellScroller?.querySelector('.select-text')?.textContent).toContain('A long value');
    expect(
      cellScroller
        ?.querySelector('.scrollbar-thin-glow-scroll')
        ?.getAttribute('tabindex'),
    ).toBeNull();
  });

  it('keeps dynamic columns readable in horizontal scroll mode', () => {
    const wideColumns: DenseColumn<Row>[] = Array.from({ length: 8 }, (_, index) => ({
      key: `column-${index}`,
      title: `Long evidence column ${index}`,
      render: (row) => row.name,
    }));
    const { container } = render(
      <DenseDataTable
        columns={wideColumns}
        rows={[{ id: 'row-1', name: 'Cell value' }]}
        getRowKey={(row) => row.id}
        horizontalScroll
        minColumnWidth={160}
      />,
    );

    const scrollContainer = getScrollViewport(container);
    const tableContainer = container.querySelector('[data-slot="table-container"]');
    const table = container.querySelector('[data-slot="table"]') as HTMLTableElement;
    const headerTitle = screen.getByText('Long evidence column 0');

    expect(scrollContainer.className).toContain('overflow-x-auto');
    expect(tableContainer?.className).toContain('overflow-visible');
    expect(table.style.width).toBe('1280px');
    expect(table.style.minWidth).toBe('100%');
    expect(headerTitle.className).toContain('truncate');
    expect(headerTitle.getAttribute('title')).toBe('Long evidence column 0');
  });

  it('keeps the header sticky to the real table scroll viewport', () => {
    const { container } = render(
      <DenseDataTable
        columns={columns}
        rows={Array.from({ length: 100 }, (_, index) => ({
          id: `row-${index}`,
          name: `Row ${index}`,
        }))}
        getRowKey={(row) => row.id}
      />,
    );

    const scrollViewport = getScrollViewport(container);
    const tableContainer = container.querySelector('[data-slot="table-container"]');
    const tableHeader = container.querySelector<HTMLElement>('[data-slot="table-header"]');

    expect(scrollViewport).toContainElement(tableHeader);
    expect(tableContainer?.className).toContain('overflow-visible');
    expect(tableContainer?.className).not.toContain('overflow-hidden');
    expect(tableHeader?.className).toContain('sticky');
    expect(tableHeader?.className).toContain('top-0');
  });

  it('does not activate a row while selecting cell text', () => {
    const onRowClick = vi.fn();
    render(
      <DenseDataTable
        columns={columns}
        rows={[{ id: 'row-1', name: 'Selectable evidence value' }]}
        getRowKey={(row) => row.id}
        onRowClick={onRowClick}
      />,
    );

    const text = screen.getByText('Selectable evidence value');
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(text);
    selection?.removeAllRanges();
    selection?.addRange(range);

    fireEvent.click(text);
    expect(onRowClick).not.toHaveBeenCalled();

    selection?.removeAllRanges();
  });

  it('requests the next bounded page near the vertical scroll boundary', () => {
    const onReachEnd = vi.fn();
    const { container } = render(
      <DenseDataTable
        columns={columns}
        rows={Array.from({ length: 100 }, (_, index) => ({
          id: `row-${index}`,
          name: `Row ${index}`,
        }))}
        getRowKey={(row) => row.id}
        hasMore
        onReachEnd={onReachEnd}
      />,
    );
    const scrollContainer = getScrollViewport(container);
    Object.defineProperties(scrollContainer, {
      clientHeight: { configurable: true, value: 600 },
      scrollHeight: { configurable: true, value: 3_100 },
    });

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });

    expect(onReachEnd).toHaveBeenCalledTimes(1);
  });

  it('continues loading when the current page does not fill the viewport', () => {
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, 'clientHeight', 'get')
      .mockReturnValue(600);
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, 'scrollHeight', 'get')
      .mockReturnValue(310);
    const onReachEnd = vi.fn();

    render(
      <DenseDataTable
        columns={columns}
        rows={Array.from({ length: 10 }, (_, index) => ({
          id: `row-${index}`,
          name: `Row ${index}`,
        }))}
        getRowKey={(row) => row.id}
        hasMore
        onReachEnd={onReachEnd}
      />,
    );

    expect(onReachEnd).toHaveBeenCalledTimes(1);
    clientHeight.mockRestore();
    scrollHeight.mockRestore();
  });

  it('restarts automatic continuation when the load context changes at the same row count', () => {
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, 'clientHeight', 'get')
      .mockReturnValue(600);
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, 'scrollHeight', 'get')
      .mockReturnValue(310);
    const onReachEnd = vi.fn();
    const firstRows = Array.from({ length: 10 }, (_, index) => ({
      id: `first-${index}`,
      name: `First ${index}`,
    }));
    const secondRows = Array.from({ length: 10 }, (_, index) => ({
      id: `second-${index}`,
      name: `Second ${index}`,
    }));
    const { rerender } = render(
      <DenseDataTable
        columns={columns}
        rows={firstRows}
        getRowKey={(row) => row.id}
        loadContextKey="query:first"
        hasMore
        onReachEnd={onReachEnd}
      />,
    );

    expect(onReachEnd).toHaveBeenCalledTimes(1);

    rerender(
      <DenseDataTable
        columns={columns}
        rows={secondRows}
        getRowKey={(row) => row.id}
        loadContextKey="query:first"
        hasMore
        onReachEnd={onReachEnd}
      />,
    );
    expect(onReachEnd).toHaveBeenCalledTimes(1);

    rerender(
      <DenseDataTable
        columns={columns}
        rows={secondRows}
        getRowKey={(row) => row.id}
        loadContextKey="query:second"
        hasMore
        onReachEnd={onReachEnd}
      />,
    );
    expect(onReachEnd).toHaveBeenCalledTimes(2);

    clientHeight.mockRestore();
    scrollHeight.mockRestore();
  });

  it('returns the virtual viewport to the first row when the load context changes', () => {
    const firstRows = Array.from({ length: 10_000 }, (_, index) => ({
      id: `first-${index}`,
      name: `First ${index}`,
    }));
    const secondRows = Array.from({ length: 10_000 }, (_, index) => ({
      id: `second-${index}`,
      name: `Second ${index}`,
    }));
    const { container, rerender } = render(
      <DenseDataTable
        columns={columns}
        rows={firstRows}
        getRowKey={(row) => row.id}
        loadContextKey="query:first"
      />,
    );
    const scrollContainer = getScrollViewport(container);

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 310_000 } });
    expect(container.textContent).toContain('First 9999');

    rerender(
      <DenseDataTable
        columns={columns}
        rows={secondRows}
        getRowKey={(row) => row.id}
        loadContextKey="query:second"
      />,
    );

    expect(scrollContainer.scrollTop).toBe(0);
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 0 } });
    expect(container.textContent).toContain('Second 0');
    expect(container.textContent).not.toContain('Second 9999');
  });

  it('uses query recovery instead of replaying a stale continuation after failure', () => {
    vi.useFakeTimers();
    const onReachEnd = vi.fn();
    const onRetryLoadMore = vi.fn();
    const rows = Array.from({ length: 100 }, (_, index) => ({
      id: `row-${index}`,
      name: `Row ${index}`,
    }));
    const { container, rerender } = render(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        hasMore
        onReachEnd={onReachEnd}
      />,
    );
    const scrollContainer = getScrollViewport(container);
    Object.defineProperties(scrollContainer, {
      clientHeight: { configurable: true, value: 600 },
      scrollHeight: { configurable: true, value: 3_100 },
    });

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });
    expect(onReachEnd).toHaveBeenCalledTimes(1);

    rerender(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        hasMore
        loadMoreFailed
        onReachEnd={onReachEnd}
        onRetryLoadMore={onRetryLoadMore}
      />,
    );
    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });

    expect(onReachEnd).toHaveBeenCalledTimes(1);
    expect(onRetryLoadMore).not.toHaveBeenCalled();
    vi.advanceTimersByTime(500);
    expect(onReachEnd).toHaveBeenCalledTimes(1);
    expect(onRetryLoadMore).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });

  it('deduplicates an explicit retry against the scheduled automatic retry', () => {
    vi.useFakeTimers();
    const onReachEnd = vi.fn();
    const onRetryLoadMore = vi.fn();

    render(
      <DenseDataTable
        columns={columns}
        rows={[{ id: 'row-1', name: 'Row 1' }]}
        getRowKey={(row) => row.id}
        hasMore
        loadMoreFailed
        onReachEnd={onReachEnd}
        onRetryLoadMore={onRetryLoadMore}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onRetryLoadMore).toHaveBeenCalledTimes(1);
    expect(onReachEnd).not.toHaveBeenCalled();

    vi.advanceTimersByTime(500);
    expect(onRetryLoadMore).toHaveBeenCalledTimes(1);
    expect(onReachEnd).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('retries a failed short page once and keeps an explicit retry control', async () => {
    vi.useFakeTimers();
    const clientHeight = vi
      .spyOn(HTMLElement.prototype, 'clientHeight', 'get')
      .mockReturnValue(600);
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, 'scrollHeight', 'get')
      .mockReturnValue(310);
    const onReachEnd = vi.fn();

    render(
      <DenseDataTable
        columns={columns}
        rows={[{ id: 'row-1', name: 'Row 1' }]}
        getRowKey={(row) => row.id}
        hasMore
        loadMoreFailed
        onReachEnd={onReachEnd}
      />,
    );

    await vi.advanceTimersByTimeAsync(500);
    expect(onReachEnd).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(2_000);
    expect(onReachEnd).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onReachEnd).toHaveBeenCalledTimes(2);

    clientHeight.mockRestore();
    scrollHeight.mockRestore();
    vi.useRealTimers();
  });

  it('unlocks continuation when loading completes without changing row count', () => {
    const onReachEnd = vi.fn();
    const rows = Array.from({ length: 100 }, (_, index) => ({
      id: `row-${index}`,
      name: `Row ${index}`,
    }));
    const { container, rerender } = render(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        hasMore
        onReachEnd={onReachEnd}
      />,
    );
    const scrollContainer = getScrollViewport(container);
    Object.defineProperties(scrollContainer, {
      clientHeight: { configurable: true, value: 600 },
      scrollHeight: { configurable: true, value: 3_100 },
    });

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });
    expect(onReachEnd).toHaveBeenCalledTimes(1);

    rerender(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        hasMore
        loadingMore
        onReachEnd={onReachEnd}
      />,
    );
    rerender(
      <DenseDataTable
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        hasMore
        onReachEnd={onReachEnd}
      />,
    );

    fireEvent.scroll(scrollContainer, { target: { scrollTop: 2_500 } });
    expect(onReachEnd).toHaveBeenCalledTimes(2);
  });

  it('renders an initial load error instead of the normal empty state', () => {
    const onRetryInitialLoad = vi.fn();

    render(
      <DenseDataTable
        columns={columns}
        rows={[]}
        getRowKey={(row) => row.id}
        emptyTitle="No matching records"
        initialLoadFailed
        initialLoadErrorText="Unable to load evidence records."
        onRetryInitialLoad={onRetryInitialLoad}
      />,
    );

    expect(screen.getByText('Unable to load evidence records.')).toBeDefined();
    expect(screen.queryByText('No matching records')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    expect(onRetryInitialLoad).toHaveBeenCalledTimes(1);
  });
});
