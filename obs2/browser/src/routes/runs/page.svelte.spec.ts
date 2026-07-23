import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import RunsPage from './+page.svelte';
import type { RunClip, RunsResponse } from '$lib/api';
import { settings } from '$lib/stores/settings.svelte';
import { youtube } from '$lib/stores/youtube.svelte';

const mocks = vi.hoisted(() => ({
	revealRunFolder: vi.fn(),
	getRuns: vi.fn(),
	keepRun: vi.fn(),
	renameRun: vi.fn(),
	deleteCatalogRun: vi.fn(),
	updateRunMetadata: vi.fn(),
	goto: vi.fn(),
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

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			revealRunFolder: mocks.revealRunFolder,
			runVideoUrl: mocks.runVideoUrl,
			keepRun: mocks.keepRun,
			getRuns: mocks.getRuns,
			renameRun: mocks.renameRun,
			deleteCatalogRun: mocks.deleteCatalogRun,
			updateRunMetadata: mocks.updateRunMetadata
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

const runsResponse: RunsResponse = {
	directories: [{ kind: 'completed', path: '/runs', exists: true, error: null }],
	clips: [
		clip({
			fileName: 'facility-0058.mov',
			timestamp: '2026-07-10T10:00:00Z',
			level: 'Facility',
			levelNumber: 2,
			difficulty: '00 Agent',
			status: 'complete',
			time: '00:58'
		}),
		clip({
			fileName: 'dam-failed.mov',
			timestamp: '2026-07-11T10:00:00Z',
			level: 'Dam',
			levelNumber: 1,
			difficulty: 'Agent',
			status: 'failed',
			time: '01:12'
		}),
		{
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
	]
};

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
	mocks.keepRun.mockImplementation(async (runId: string) => {
		const run = runsResponse.clips.find((candidate) => candidate.runId === runId);
		if (!run) throw new Error('run not found');
		return { ...run, retentionState: 'kept' };
	});
	mocks.getRuns.mockResolvedValue(runsResponse);
	mocks.renameRun.mockImplementation(async (path: string, fileName: string) => {
		const run = runsResponse.clips.find((candidate) => candidate.path === path);
		if (!run) throw new Error('run not found');
		return { ...run, path: `/runs/${fileName}`, fileName };
	});
	mocks.deleteCatalogRun.mockResolvedValue(null);
	mocks.updateRunMetadata.mockImplementation(async (runId: string, metadata) => {
		const run = runsResponse.clips.find((candidate) => candidate.runId === runId);
		if (!run) throw new Error('run not found');
		return {
			...run,
			metadata: {
				...run.metadata,
				...metadata,
				timeSeconds: metadata.time
					? Number(metadata.time.split(':')[0]) * 60 + Number(metadata.time.split(':')[1])
					: undefined
			}
		};
	});
});

describe('/runs', () => {
	it('passes refresh=true when the user reloads runs', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		const reload = await screen.findByRole('button', { name: /reload/i });
		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(1));
		expect(mocks.getRuns.mock.calls[0][0]).toMatchObject({ refresh: false, sort: 'newest' });

		await user.click(reload);
		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(2));
		expect(mocks.getRuns.mock.calls[1][0]).toMatchObject({ refresh: true, sort: 'newest' });
	});

	it('keeps search visible and applied when the secondary filters are collapsed', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		await screen.findByRole('button', { name: /facility-0058\.mov/i });
		await user.type(screen.getByRole('searchbox', { name: /search runs/i }), 'facility');

		await waitFor(() => expect(screen.queryByRole('button', { name: /dam-failed\.mov/i })).not.toBeInTheDocument());

		await user.click(screen.getByRole('button', { name: /filters/i }));
		expect(screen.getByRole('combobox', { name: 'Level' })).toBeInTheDocument();
		await user.click(screen.getByRole('button', { name: /filters/i }));

		expect(screen.getByRole('searchbox', { name: /search runs/i })).toHaveValue('facility');
		expect(screen.queryByRole('combobox', { name: 'Level' })).not.toBeInTheDocument();
		expect(screen.getByText('search: facility')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /facility-0058\.mov/i })).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /dam-failed\.mov/i })).not.toBeInTheDocument();
	});

	it('sorts by run time and requests the same order from the catalog', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		await screen.findByRole('button', { name: /facility-0058\.mov/i });
		await user.click(screen.getByRole('button', { name: /Sort runs, current: Newest first/i }));
		await user.click(screen.getByRole('menuitemradio', { name: 'Fastest first' }));

		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(2));
		expect(mocks.getRuns.mock.calls[1][0]).toMatchObject({ refresh: false, sort: 'fastest' });
		const runButtons = screen.getAllByRole('button', { name: /^Open /i });
		expect(runButtons.map((button) => button.getAttribute('aria-label'))).toEqual([
			'Open facility-0058.mov',
			'Open Archives run history only',
			'Open dam-failed.mov'
		]);
		expect(mocks.goto).toHaveBeenCalledWith('/runs?sort=fastest', {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
	});

	it('restores the selected sort order from the URL', async () => {
		mocks.pageUrl = new URL('http://localhost/runs?sort=slowest');
		window.history.replaceState({}, '', '/runs?sort=slowest');
		render(RunsPage);

		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(1));
		expect(mocks.getRuns.mock.calls[0][0]).toMatchObject({ refresh: false, sort: 'slowest' });
		expect(await screen.findByRole('button', { name: /Sort runs, current: Slowest first/i })).toBeInTheDocument();
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
		const dialog = screen.getByRole('dialog', { name: 'Run video' });
		expect(within(dialog).getByText('Size')).toBeInTheDocument();
		expect(within(dialog).getByText('1 KB')).toBeInTheDocument();
		expect(within(dialog).getByText('Retention state')).toBeInTheDocument();
		expect(within(dialog).getByText('Kept')).toBeInTheDocument();
		expect(within(dialog).getByText('Retention reason')).toBeInTheDocument();
		expect(within(dialog).getByText('Kept manually')).toBeInTheDocument();
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
		expect(screen.getByText('History only')).toBeInTheDocument();
		expect(screen.getByText('Video deleted')).toBeInTheDocument();
	});

	it('explains pending cleanup in the modal and lets the user keep the video', async () => {
		const user = userEvent.setup();
		const pending = {
			...clip({
				fileName: 'facility-pending.mov',
				timestamp: '2026-07-13T10:00:00Z',
				level: 'Facility',
				levelNumber: 2,
				difficulty: '00 Agent',
				status: 'complete',
				time: '00:55'
			}),
			retentionState: 'pending' as const
		};
		mocks.getRuns.mockResolvedValue({ directories: runsResponse.directories, clips: [pending] });
		mocks.keepRun.mockResolvedValue({ ...pending, retentionState: 'kept' });
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-pending\.mov/i }));
		expect(screen.getByRole('region', { name: 'Pending video retention' })).toBeInTheDocument();
		expect(screen.getByText(/deleted when it falls outside your recent-run history/i)).toBeInTheDocument();
		await user.click(screen.getByRole('button', { name: 'keep video' }));

		expect(mocks.keepRun).toHaveBeenCalledWith('facility-pending.mov');
		await waitFor(() =>
			expect(screen.queryByRole('region', { name: 'Pending video retention' })).not.toBeInTheDocument()
		);
	});

	it('opens the requested run from a recent-run link', async () => {
		mocks.pageUrl = new URL('http://localhost/runs?runId=deleted-run');
		window.history.replaceState({}, '', '/runs?runId=deleted-run');
		render(RunsPage);

		expect(await screen.findByRole('dialog', { name: 'Run video' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Archives run history' })).toBeInTheDocument();
	});

	it('keeps the list and modal in sync after renaming by run ID', async () => {
		const user = userEvent.setup();
		vi.spyOn(window, 'prompt').mockReturnValue('facility-renamed.mov');
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-0058\.mov/i }));
		await user.click(within(screen.getByRole('dialog', { name: 'Run video' })).getByRole('button', { name: 'rename' }));

		expect(mocks.renameRun).toHaveBeenCalledWith('/runs/facility-0058.mov', 'facility-renamed.mov');
		expect(await screen.findByRole('heading', { name: 'facility-renamed.mov' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /Open facility-renamed\.mov/i })).toBeInTheDocument();
	});

	it('deletes only the video when the user chooses to keep run history', async () => {
		const user = userEvent.setup();
		const original = runsResponse.clips[0];
		mocks.deleteCatalogRun.mockResolvedValue({
			...original,
			path: '',
			fileName: '',
			directory: '',
			sizeBytes: 0,
			durationSecs: null,
			retentionState: 'expired',
			retentionReason: 'deleted'
		});
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-0058\.mov/i }));
		await user.click(within(screen.getByRole('dialog', { name: 'Run video' })).getByRole('button', { name: 'delete' }));
		await user.click(screen.getByRole('button', { name: 'Delete video, keep run history' }));

		expect(mocks.deleteCatalogRun).toHaveBeenCalledWith(original.runId, true);
		expect(await screen.findByText('The video has been removed. Run history is still available.')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /Open Facility run history only/i })).toBeInTheDocument();
	});

	it('removes the row when the user deletes the video and run history', async () => {
		const user = userEvent.setup();
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /dam-failed\.mov/i }));
		await user.click(within(screen.getByRole('dialog', { name: 'Run video' })).getByRole('button', { name: 'delete' }));
		await user.click(screen.getByRole('button', { name: 'Delete video and run history' }));

		expect(mocks.deleteCatalogRun).toHaveBeenCalledWith('dam-failed.mov', false);
		await waitFor(() => expect(screen.queryByRole('button', { name: /dam-failed\.mov/i })).not.toBeInTheDocument());
	});

	it('flushes edits made during an in-flight metadata save before closing', async () => {
		const user = userEvent.setup();
		let resolveFirst!: (value: RunClip) => void;
		const original = runsResponse.clips[0];
		mocks.updateRunMetadata
			.mockImplementationOnce(
				() =>
					new Promise<RunClip>((resolve) => {
						resolveFirst = resolve;
					})
			)
			.mockImplementationOnce(async (_runId: string, metadata) => ({
				...original,
				metadata: { ...original.metadata, ...metadata }
			}));
		render(RunsPage);

		await user.click(await screen.findByRole('button', { name: /facility-0058\.mov/i }));
		const time = screen.getByRole('textbox', { name: 'Time' });
		await user.clear(time);
		await user.type(time, '01:00');
		await user.tab();
		await waitFor(() => expect(mocks.updateRunMetadata).toHaveBeenCalledTimes(1));

		await user.clear(time);
		await user.type(time, '01:01');
		await user.click(within(screen.getByRole('dialog', { name: 'Run video' })).getByRole('button', { name: 'close' }));
		resolveFirst({ ...original, metadata: { ...original.metadata, time: '01:00', timeSeconds: 60 } });

		await waitFor(() => expect(mocks.updateRunMetadata).toHaveBeenCalledTimes(2));
		expect(mocks.updateRunMetadata.mock.calls[1][1]).toMatchObject({ time: '01:01' });
		await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Run video' })).not.toBeInTheDocument());
	});
});
