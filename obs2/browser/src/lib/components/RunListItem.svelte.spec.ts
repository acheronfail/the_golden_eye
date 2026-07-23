import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import RunListItem from './RunListItem.svelte';
import type { RunClip } from '$lib/api';
import { formatDate } from '$lib/utils/runsView';

const clip: RunClip = {
	path: '/runs/facility.mov',
	fileName: 'facility.mov',
	directory: '/runs',
	sizeBytes: 1024,
	modified: '2026-07-21T12:45:04Z',
	durationSecs: 75.4,
	metadata: {
		timestamp: '2026-07-21T12:43:09Z',
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
	}
};

describe('RunListItem', () => {
	it('stacks the run time with difficulty and exposes the full timestamp on hover', () => {
		render(RunListItem, {
			clip,
			fileBrowserLabel: 'Show in Finder',
			open: () => {},
			rename: () => {},
			reveal: () => {},
			remove: () => {}
		});

		const time = screen.getByText('00:58');
		expect(time.parentElement).toContainElement(screen.getByText('00 Agent'));
		expect(screen.getByTitle(formatDate(clip.metadata.timestamp))).toBeInTheDocument();
	});

	it('highlights pending retention in red', () => {
		render(RunListItem, {
			clip: { ...clip, retentionState: 'pending' },
			fileBrowserLabel: 'Show in Finder',
			open: () => {},
			rename: () => {},
			reveal: () => {},
			remove: () => {}
		});

		expect(screen.getByText('pending')).toHaveClass('text-(--obs-danger)');
	});

	it.each([
		['Open', 'open'],
		['Rename', 'rename'],
		['Show in Finder', 'reveal'],
		['Delete', 'remove']
	] as const)('exposes the %s action from the more menu', async (label, callbackName) => {
		const user = userEvent.setup();
		const callbacks = {
			open: vi.fn(),
			rename: vi.fn(),
			reveal: vi.fn(),
			remove: vi.fn()
		};
		render(RunListItem, { clip, fileBrowserLabel: 'Show in Finder', ...callbacks });

		await user.click(screen.getByRole('button', { name: 'More actions' }));
		await user.click(screen.getByRole('menuitem', { name: label }));

		expect(callbacks[callbackName]).toHaveBeenCalledWith(clip);
		expect(screen.queryByRole('menu')).not.toBeInTheDocument();
	});
});
