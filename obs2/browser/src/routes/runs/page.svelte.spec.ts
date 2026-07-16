import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RunsPage from './+page.svelte';
import type { RunClip, RunsStreamEvent } from '$lib/api';
import { settings } from '$lib/settings.svelte';

const mocks = vi.hoisted(() => ({
	revealRunFolder: vi.fn(),
	streamRuns: vi.fn(),
	runVideoUrl: vi.fn((path: string) => `/api/v1/runs/video?path=${encodeURIComponent(path)}`)
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		revealRunFolder: mocks.revealRunFolder,
		runVideoUrl: mocks.runVideoUrl,
		streamRuns: mocks.streamRuns
	};
});

const clip = (overrides: {
	fileName: string;
	timestamp: string;
	level: string;
	levelNumber: number;
	difficulty: string;
	status: string;
	time: string;
}): RunClip => ({
	path: `/runs/${overrides.fileName}`,
	fileName: overrides.fileName,
	directory: '/runs',
	sizeBytes: 1024,
	modified: overrides.timestamp,
	durationSecs: 70,
	metadata: {
		timestamp: overrides.timestamp,
		time: overrides.time,
		timeSeconds: undefined,
		level: overrides.level,
		levelNumber: overrides.levelNumber,
		difficulty: overrides.difficulty,
		status: overrides.status,
		romLanguage: 'en',
		sourceName: 'GoldenEye',
		comment: 'The Golden Eye',
		pluginVersion: '1.0.0'
	}
});

const streamEvents: RunsStreamEvent[] = [
	{
		type: 'directory',
		directory: { kind: 'completed', path: '/runs', exists: true, error: null }
	},
	{
		type: 'directory',
		directory: { kind: 'failed', path: '/runs/failed', exists: true, error: null }
	},
	{
		type: 'clip',
		clip: clip({
			fileName: 'facility-0058.mov',
			timestamp: '2026-07-10T10:00:00Z',
			level: 'Facility',
			levelNumber: 2,
			difficulty: '00 Agent',
			status: 'complete',
			time: '00:58'
		})
	},
	{
		type: 'clip',
		clip: clip({
			fileName: 'dam-failed.mov',
			timestamp: '2026-07-11T10:00:00Z',
			level: 'Dam',
			levelNumber: 1,
			difficulty: 'Agent',
			status: 'failed',
			time: '01:12'
		})
	},
	{ type: 'done' }
];

beforeEach(() => {
	vi.clearAllMocks();
	settings.saveFailedRuns = true;
	mocks.revealRunFolder.mockResolvedValue(undefined);
	mocks.streamRuns.mockImplementation(async (onEvent: (event: RunsStreamEvent) => void) => {
		for (const event of streamEvents) onEvent(event);
	});
});

describe('/runs', () => {
	it('keeps filters applied when the filter controls are collapsed', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		await screen.findByRole('button', { name: /facility-0058\.mov/i });
		await user.type(screen.getByRole('searchbox', { name: /search runs/i }), 'facility');

		await waitFor(() => expect(screen.queryByRole('button', { name: /dam-failed\.mov/i })).not.toBeInTheDocument());

		await user.click(screen.getByRole('button', { name: /filters/i }));

		expect(screen.queryByRole('searchbox', { name: /search runs/i })).not.toBeInTheDocument();
		expect(screen.getByText('search: facility')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /facility-0058\.mov/i })).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /dam-failed\.mov/i })).not.toBeInTheDocument();
	});

	it('lets the user choose which configured clips folder to open', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		const showFolder = await screen.findByRole('button', { name: /show clips/i });
		await user.click(showFolder);
		await user.click(screen.getByRole('button', { name: /Failed clips/i }));

		expect(mocks.revealRunFolder).toHaveBeenCalledWith('failed');
	});

	it('opens completed clips directly when failed run saving is disabled', async () => {
		const user = userEvent.setup();
		settings.saveFailedRuns = false;
		render(RunsPage);

		const showFolder = await screen.findByRole('button', { name: /show clips/i });
		await user.click(showFolder);

		expect(screen.queryByRole('dialog', { name: /Choose clips folder/i })).not.toBeInTheDocument();
		expect(mocks.revealRunFolder).toHaveBeenCalledWith('completed');
	});
});
