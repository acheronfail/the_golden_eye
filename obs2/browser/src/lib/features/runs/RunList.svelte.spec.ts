import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import RunList from './RunList.svelte';
import type { RunClip } from '$lib/api';

const runClip = (fileName: string, level: string): RunClip => ({
	runId: fileName,
	path: `/runs/${fileName}`,
	fileName,
	directory: '/runs',
	sizeBytes: 1024,
	modified: '2026-07-21T12:45:04Z',
	durationSecs: 75.4,
	metadata: {
		timestamp: '2026-07-21T12:43:09Z',
		time: '00:58',
		timeSeconds: 58,
		level,
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		gameLanguage: 'en',
		sourceName: 'Nintendo 64',
		comment: '',
		pluginVersion: '2.4.0'
	},
	retentionState: 'kept',
	retentionReason: 'manual'
});

describe('RunList', () => {
	it('only mounts the visible window for long run histories', () => {
		const clips = Array.from({ length: 100 }, (_, index) => runClip(`run-${index}.mov`, `Run ${index}`));
		const { container } = render(RunList, {
			loading: false,
			clips,
			visibleClips: clips,
			scannedDirectoryCount: 1,
			directoryCount: 1,
			hasActiveFilters: false,
			sort: 'newest',
			onSortChange: () => {},
			fileBrowserLabel: 'Show in Finder',
			clearFilters: () => {},
			open: () => {},
			rename: () => {},
			reveal: () => {},
			remove: () => {}
		});

		expect(container.querySelectorAll('[role="listitem"]').length).toBeGreaterThan(0);
		expect(container.querySelectorAll('[role="listitem"]').length).toBeLessThan(clips.length);
		expect(container.querySelector('[role="list"] > [aria-hidden="true"]')).toHaveClass('h-[var(--list-height)]');
	});

	it('updates the visible window when the app content container scrolls', async () => {
		const clips = Array.from({ length: 100 }, (_, index) => runClip(`run-${index}.mov`, `Run ${index}`));
		let listTop = 100;
		document.body.classList.add('obs-content-scroller');
		const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
			if (this.getAttribute('role') === 'list') {
				return { top: listTop, bottom: listTop + 5638, height: 5638 } as DOMRect;
			}
			if (this === document.body) return { top: 0, bottom: 600, height: 600 } as DOMRect;
			return { top: 0, bottom: 0, height: 0 } as DOMRect;
		});

		try {
			render(RunList, {
				loading: false,
				clips,
				visibleClips: clips,
				scannedDirectoryCount: 1,
				directoryCount: 1,
				hasActiveFilters: false,
				sort: 'fastest',
				onSortChange: () => {},
				fileBrowserLabel: 'Show in Finder',
				clearFilters: () => {},
				open: () => {},
				rename: () => {},
				reveal: () => {},
				remove: () => {}
			});
			expect(screen.queryByRole('button', { name: 'Open run-50.mov' })).not.toBeInTheDocument();

			listTop = -2800;
			document.body.dispatchEvent(new Event('scroll'));

			await waitFor(() => expect(screen.getByRole('button', { name: 'Open run-50.mov' })).toBeInTheDocument());
		} finally {
			rect.mockRestore();
			document.body.classList.remove('obs-content-scroller');
		}
	});

	it('keeps one action menu open and dismisses it on an outside click', async () => {
		const user = userEvent.setup();
		const clips = [runClip('facility.mov', 'Facility'), runClip('control.mov', 'Control')];
		render(RunList, {
			loading: false,
			clips,
			visibleClips: clips,
			scannedDirectoryCount: 2,
			directoryCount: 2,
			hasActiveFilters: false,
			sort: 'newest',
			onSortChange: () => {},
			fileBrowserLabel: 'Show in Finder',
			clearFilters: () => {},
			open: () => {},
			rename: () => {},
			reveal: () => {},
			remove: () => {}
		});

		const triggers = screen.getAllByRole('button', { name: 'More actions' });
		await user.click(triggers[0]);
		expect(screen.getByRole('menu', { name: 'Actions for facility.mov' })).toBeInTheDocument();

		await user.click(triggers[1]);
		expect(screen.queryByRole('menu', { name: 'Actions for facility.mov' })).not.toBeInTheDocument();
		expect(screen.getByRole('menu', { name: 'Actions for control.mov' })).toBeInTheDocument();

		await user.click(document.body);
		expect(screen.queryByRole('menu')).not.toBeInTheDocument();
	});
});
