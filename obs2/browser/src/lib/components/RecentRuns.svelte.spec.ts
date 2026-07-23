import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { RunClip } from '$lib/api';
import RecentRuns from './RecentRuns.svelte';

const run = (overrides: Partial<RunClip>): RunClip => ({
	runId: 'run-1',
	path: '',
	fileName: '',
	directory: '',
	sizeBytes: 0,
	metadata: {
		runId: 'run-1',
		timestamp: '2026-07-23T10:00:00Z',
		time: '00:58',
		timeSeconds: 58,
		level: 'Facility',
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		romLanguage: 'en',
		sourceName: 'Nintendo 64',
		comment: '',
		pluginVersion: '2.4.0'
	},
	retentionState: 'pending',
	retentionReason: null,
	...overrides
});

describe('RecentRuns', () => {
	it('shows a finalized run while its clip is still saving', () => {
		render(RecentRuns, { runs: [run({})], onKeep: vi.fn() });

		expect(screen.getByRole('link', { name: 'Facility' })).toHaveAttribute('href', '/runs?runId=run-1');
		expect(screen.getByText('complete')).toHaveClass('status-complete');
		expect(screen.getByRole('article')).not.toHaveTextContent('•');
		expect(screen.getByText('Saving…')).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: 'Keep' })).not.toBeInTheDocument();
	});

	it('colors failed statuses and personal-best times', () => {
		render(RecentRuns, {
			runs: [
				run({
					retentionState: 'kept',
					retentionReason: 'personalBest',
					metadata: { ...run({}).metadata, status: 'failed' }
				})
			],
			onKeep: vi.fn()
		});

		expect(screen.getByText('failed')).toHaveClass('status-failed');
		expect(screen.getByText('00:58')).toHaveClass('personal-best');
		expect(screen.getByText('PB')).toHaveClass('row-action', 'state', 'kept');
	});

	it.each(['failed', 'abort', 'kia'])('colors the %s outcome as failed', (status) => {
		render(RecentRuns, {
			runs: [run({ metadata: { ...run({}).metadata, status } })],
			onKeep: vi.fn()
		});

		expect(screen.getByText(status)).toHaveClass('status-failed');
	});

	it('adds Keep to the same row after its video is available', async () => {
		const user = userEvent.setup();
		const onKeep = vi.fn();
		render(RecentRuns, { runs: [run({ path: '/runs/facility.mov', fileName: 'facility.mov' })], onKeep });

		await user.click(screen.getByRole('button', { name: 'Keep' }));
		expect(onKeep).toHaveBeenCalledWith('run-1');
		expect(screen.getByRole('button', { name: 'Keep' })).toHaveClass('row-action');
	});

	it('scrolls back to the newest run when one is prepended', async () => {
		const older = run({ runId: 'older' });
		const view = render(RecentRuns, { runs: [older], onKeep: vi.fn() });
		const scroll = view.container.querySelector('.run-scroll') as HTMLDivElement;
		await waitFor(() => expect(scroll.scrollTop).toBe(0));
		scroll.scrollTop = 100;

		await view.rerender({
			runs: [run({ runId: 'newest', metadata: { ...older.metadata, timestamp: '2026-07-23T10:01:00Z' } }), older],
			onKeep: vi.fn()
		});

		await waitFor(() => expect(scroll.scrollTop).toBe(0));
	});
});
