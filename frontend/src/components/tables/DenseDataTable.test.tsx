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

    const scrollContainer = container.firstElementChild as HTMLDivElement;

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
});
