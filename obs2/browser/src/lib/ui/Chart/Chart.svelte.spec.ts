import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import Chart from './Chart.svelte';

vi.stubGlobal(
	'ResizeObserver',
	class {
		observe() {}
		disconnect() {}
	}
);

describe('Chart', () => {
	it('uses the missing-preview treatment when there is no chart data', () => {
		render(Chart, {
			title: 'Empty chart',
			description: 'An empty chart',
			data: { kind: 'line', xType: 'time', series: [] }
		});

		expect(screen.getByText('No data in this range').parentElement).toHaveClass('obs-preview-missing');
	});

	it('uses clickable legend items to control visible series', async () => {
		const user = userEvent.setup();
		const onVisibleSeriesChange = vi.fn();
		const { container } = render(Chart, {
			title: 'PB progression',
			description: 'Personal-best progression and attempts',
			data: {
				kind: 'line',
				xType: 'time',
				series: [
					{
						id: 'running-best',
						label: 'Personal best',
						color: 'var(--obs-gold)',
						lineStyle: 'step',
						renderPriority: 1,
						points: [
							{ x: 1, y: 80 },
							{ x: 2, y: 70 }
						]
					},
					{
						id: 'complete',
						label: 'Complete',
						color: 'var(--obs-success)',
						lineStyle: 'none',
						points: [{ x: 2, y: 75 }]
					},
					{
						id: 'kia',
						label: 'Killed in Action',
						color: 'var(--obs-danger)',
						lineStyle: 'none',
						points: []
					}
				],
				referenceLines: [
					{
						id: 'personal-best',
						value: 70,
						color: 'var(--obs-gold)',
						seriesId: 'running-best'
					}
				]
			},
			interactiveLegend: true,
			visibleSeriesIds: ['running-best', 'complete'],
			onVisibleSeriesChange
		});

		expect(screen.getByRole('button', { name: 'Hide Personal best' })).toHaveAttribute('aria-pressed', 'true');
		const referenceLine = container.querySelector('[data-chart-reference-line="personal-best"]');
		expect(referenceLine).toHaveAttribute('stroke-width', '1');
		expect(referenceLine).toHaveAttribute('stroke-dasharray', '0.5 2.5');
		expect(container.querySelector('svg')).toHaveAttribute('aria-label', 'PB progression');
		expect(container.querySelector('svg title')).not.toBeInTheDocument();
		expect([...container.querySelectorAll('[data-chart-series]')].at(-1)).toHaveAttribute(
			'data-chart-series',
			'running-best'
		);
		const complete = screen.getByRole('button', { name: 'Hide Complete' });
		expect(complete).not.toHaveClass('line-through');
		expect(screen.getByRole('button', { name: 'Show Killed in Action' })).toBeInTheDocument();

		await user.click(complete);
		expect(onVisibleSeriesChange).toHaveBeenCalledWith(['running-best']);
	});

	it.each([
		['groupedBar', 'Failed, Complete: 4'],
		['stackedBar', 'Failed, Complete: 4']
	] as const)('renders accessible marks for %s charts', (kind, accessibleName) => {
		render(Chart, {
			title: 'Attempts',
			description: 'Attempts by outcome',
			data: {
				kind,
				xType: 'category',
				series: [
					{
						id: 'complete',
						label: 'Complete',
						color: 'green',
						points: [{ x: 'Failed', y: 4 }]
					}
				]
			}
		});

		expect(screen.getByRole('button', { name: accessibleName })).toBeInTheDocument();
	});

	it('renders accessible horizontal stacked-bar marks and category labels', () => {
		render(Chart, {
			title: 'Attempts by level',
			description: 'Attempts split by outcome',
			data: {
				kind: 'horizontalStackedBar',
				xType: 'category',
				series: [
					{
						id: 'complete',
						label: 'Complete',
						color: 'green',
						points: [{ x: 'Dam', y: 4 }]
					}
				]
			}
		});

		expect(screen.getByRole('button', { name: 'Dam, Complete: 4' })).toBeInTheDocument();
		expect(screen.getByText('Dam')).toBeInTheDocument();
	});

	it('renders accessible horizontal grouped-bar marks and category labels', () => {
		render(Chart, {
			title: 'Attempts by level',
			description: 'Attempts split by difficulty',
			data: {
				kind: 'horizontalGroupedBar',
				xType: 'category',
				series: [
					{
						id: 'agent',
						label: 'Agent',
						color: 'green',
						points: [{ x: 'Dam', y: 4 }]
					}
				]
			}
		});

		expect(screen.getByRole('button', { name: 'Dam, Agent: 4' })).toBeInTheDocument();
		expect(screen.getByText('Dam')).toBeInTheDocument();
	});
});
