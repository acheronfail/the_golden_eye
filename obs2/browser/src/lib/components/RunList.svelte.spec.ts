import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
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
		romLanguage: 'en',
		sourceName: 'Nintendo 64',
		comment: '',
		pluginVersion: '2.4.0'
	},
	retentionState: 'kept',
	retentionReason: 'manual'
});

describe('RunList', () => {
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
