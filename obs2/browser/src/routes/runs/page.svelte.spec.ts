import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RunsPage from './+page.svelte';
import type { RunClip, RunsStreamEvent } from '$lib/api';
import { settings } from '$lib/stores/settings.svelte';
import { youtube } from '$lib/stores/youtube.svelte';

const mocks = vi.hoisted(() => ({
	revealRunFolder: vi.fn(),
	streamRuns: vi.fn(),
	runVideoUrl: vi.fn((path: string) => `/api/v1/runs/video?path=${encodeURIComponent(path)}`),
	pageUrl: new URL('http://localhost/runs')
}));

vi.mock('$app/state', () => ({
	page: {
		get url() {
			return mocks.pageUrl;
		}
	}
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			revealRunFolder: mocks.revealRunFolder,
			runVideoUrl: mocks.runVideoUrl,
			streamRuns: mocks.streamRuns
		}
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
	runId: overrides.fileName,
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
	},
	retentionState: 'kept',
	retentionReason: 'manual'
});

const streamEvents: RunsStreamEvent[] = [
	{
		type: 'directory',
		directory: { kind: 'completed', path: '/runs', exists: true, error: null }
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
	{
		type: 'clip',
		clip: {
			...clip({
				fileName: 'deleted-run',
				timestamp: '2026-07-12T10:00:00Z',
				level: 'Archives',
				levelNumber: 10,
				difficulty: 'Secret Agent',
				status: 'failed',
				time: '01:05'
			}),
			path: '',
			fileName: '',
			directory: '',
			sizeBytes: 0,
			durationSecs: null,
			retentionState: 'expired',
			retentionReason: 'deleted'
		}
	},
	{ type: 'done' }
];

beforeEach(() => {
	vi.clearAllMocks();
	mocks.pageUrl = new URL('http://localhost/runs');
	window.history.replaceState({}, '', '/runs');
	youtube.loaded = true;
	youtube.enabled = false;
	youtube.oauthConfigured = false;
	youtube.connected = false;
	youtube.account = null;
	youtube.uploads = [];
	youtube.history = [];
	mocks.revealRunFolder.mockResolvedValue(undefined);
	mocks.streamRuns.mockImplementation(async (onEvent: (event: RunsStreamEvent) => void) => {
		for (const event of streamEvents) onEvent(event);
	});
});

describe('/runs', () => {
	it('passes refresh=true when the user reloads runs', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		const reload = await screen.findByRole('button', { name: /reload/i });
		await waitFor(() => expect(mocks.streamRuns).toHaveBeenCalledTimes(1));
		expect(mocks.streamRuns.mock.calls[0][2]).toEqual({ refresh: false });

		await user.click(reload);
		await waitFor(() => expect(mocks.streamRuns).toHaveBeenCalledTimes(2));
		expect(mocks.streamRuns.mock.calls[1][2]).toEqual({ refresh: true });
	});

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

	it('opens the standard clips folder directly', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		const showFolder = await screen.findByRole('button', { name: /show clips/i });
		await user.click(showFolder);

		expect(screen.queryByRole('dialog', { name: /Choose clips folder/i })).not.toBeInTheDocument();
		expect(mocks.revealRunFolder).toHaveBeenCalledWith('completed');
	});

	it('prompts for a YouTube connection without showing the upload preview', async () => {
		const user = userEvent.setup();
		youtube.enabled = true;
		youtube.oauthConfigured = true;
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-0058\.mov/i }));

		expect(screen.getByRole('heading', { name: 'YouTube' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Metadata' })).toBeInTheDocument();
		expect(screen.getByText('Connect YouTube to upload videos.')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Connect YouTube' })).toBeInTheDocument();
		expect(screen.queryByText('Preview')).not.toBeInTheDocument();
	});

	it('shows the YouTube Preview after connecting', async () => {
		const user = userEvent.setup();
		youtube.enabled = true;
		youtube.oauthConfigured = true;
		youtube.connected = true;
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-0058\.mov/i }));

		expect(screen.getByText('Preview')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Edit templates' })).toBeInTheDocument();
		expect(screen.queryByText('Connect YouTube to upload videos.')).not.toBeInTheDocument();
	});

	it('keeps metadata editable for run history whose video was deleted', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /Run history only/i }));

		expect(screen.getByText('The video has been removed. Run history is still available.')).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Metadata' })).toBeInTheDocument();
		expect(screen.getByRole('textbox', { name: 'Time' })).toHaveValue('01:05');
		expect(screen.queryByText('Size')).not.toBeInTheDocument();
	});

	it('opens the requested run from a recent-run link', async () => {
		mocks.pageUrl = new URL('http://localhost/runs?runId=deleted-run');
		window.history.replaceState({}, '', '/runs?runId=deleted-run');
		render(RunsPage);

		expect(await screen.findByRole('dialog', { name: 'Run video' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Archives run history' })).toBeInTheDocument();
	});
});
