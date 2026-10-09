import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SourcePage from './+page.svelte';
import { applyMonitorSnapshot, monitor } from '$lib/stores/monitor.svelte';
import type { AppSnapshot, LevelMatch } from '$lib/api';
import { settings } from '$lib/stores/settings.svelte';
import { obsSources } from '$lib/stores/sources.svelte';

const mocks = vi.hoisted(() => {
	const api = {
		getBestTimes: vi.fn(),
		getReplayBufferStatus: vi.fn(),
		startMonitor: vi.fn(),
		stopMonitor: vi.fn(),
		putSettings: vi.fn()
	};
	return {
		afterNavigate: vi.fn((callback: () => unknown) => {
			queueMicrotask(() => {
				void callback();
			});
		}),
		api,
		goto: vi.fn(),
		page: { url: new URL('http://localhost/sources/N64%20Capture') }
	};
});

vi.mock('$app/environment', () => ({
	browser: true,
	building: false,
	dev: false,
	version: 'test'
}));

vi.mock('$app/navigation', () => ({
	afterNavigate: mocks.afterNavigate,
	goto: mocks.goto
}));

vi.mock('$app/state', () => ({
	page: mocks.page
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			getBestTimes: mocks.api.getBestTimes,
			getReplayBufferStatus: mocks.api.getReplayBufferStatus,
			startMonitor: mocks.api.startMonitor,
			stopMonitor: mocks.api.stopMonitor,
			putSettings: mocks.api.putSettings
		}
	};
});

const monitorSnapshot = (match: LevelMatch | null = null): AppSnapshot => ({
	monitor: {
		enabled: true,
		sourceName: 'N64 Capture',
		wallClocks: {
			sessionStartedAtUnixMs: null,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: null,
			levelElapsedMs: 0,
			levelRunning: false,
			levelPaused: false,
			levelStartReason: null,
			levelTimerPhase: 'idle',
			introSwirlDelayMs: null,
			fadeDetection: null
		}
	},
	match,
	recordingState: match ? 'started' : null,
	replaySaves: [],
	runCatalogSync: null,
	sources: [],
	replayBuffer: {
		enabled: true,
		available: true,
		active: true,
		maxSeconds: 1200,
		outputDirectory: '/captures',
		defaultCompletedOutputPath: '/captures/GoldenEye'
	},
	settingsStatus: {
		settings: settings.defaults,
		defaults: settings.defaults,
		configPath: '/tmp/the-golden-eye/settings.json',
		pluginVersion: 'test',
		fileError: null
	},
	update: { phase: 'idle', available: null }
});

beforeEach(() => {
	vi.clearAllMocks();
	mocks.api.getBestTimes.mockResolvedValue([]);
	mocks.page.url = new URL('http://localhost/sources/N64%20Capture');
	obsSources.items = [{ name: 'N64 Capture', id: 'video_capture_device' }];
	obsSources.loaded = true;
	applyMonitorSnapshot(monitorSnapshot());
	monitor.chromePhase = null;
	settings.applyReloaded(
		{
			...settings.defaults,
			stopReplayBufferWhenMonitorStopped: false,
			stopReplayBufferPromptShown: true,
			welcomeModalShown: true
		},
		'/tmp/the-golden-eye/settings.json'
	);
	mocks.api.getReplayBufferStatus.mockResolvedValue({
		enabled: true,
		available: true,
		active: true,
		maxSeconds: 1200,
		outputDirectory: '/captures',
		defaultCompletedOutputPath: '/captures/GoldenEye',
		defaultFailedOutputPath: '/captures/GoldenEye/failed'
	});
	mocks.api.startMonitor.mockResolvedValue(undefined);
	mocks.api.stopMonitor.mockResolvedValue(undefined);
	mocks.api.putSettings.mockImplementation(async (next) => next);
});

describe('/sources/[sourceName]', () => {
	it.each(['mission-glass', 'signal-band'] as const)('retains the catalog PB when returning in %s', async (design) => {
		const start: LevelMatch = { screen: 'start', mission: 1, part: 2, difficulty: 2, times: null, runtime_ms: 1 };
		applyMonitorSnapshot(monitorSnapshot(start));
		mocks.api.getBestTimes.mockResolvedValue([
			{
				path: '',
				metadata: { level: 'Facility', difficulty: '00 Agent', status: 'complete', timeSeconds: 58 }
			}
		]);
		const view = render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });
		expect(await screen.findByText('Personal Best')).toBeInTheDocument();
		expect(await screen.findByText('0:58')).toBeInTheDocument();
		expect(mocks.api.getBestTimes).toHaveBeenCalled();
		view.unmount();

		settings.values.monitorDesign = design;
		applyMonitorSnapshot(monitorSnapshot({ ...start, screen: 'unknown', mission: -1, part: -1, difficulty: -1 }));
		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });
		expect(await screen.findByText('Facility / 00 Agent')).toHaveAttribute('data-available', 'true');
		expect(await screen.findByText('Personal Best')).toBeInTheDocument();
		expect(await screen.findByText('0:58')).toBeInTheDocument();
		expect(mocks.api.startMonitor).not.toHaveBeenCalled();
	});

	it('reuses an active monitor when its snapshot arrives after the page mounts', async () => {
		monitor.status = null;
		monitor.loaded = false;
		obsSources.items = null;
		obsSources.loaded = false;

		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		await Promise.resolve();
		expect(mocks.api.startMonitor).not.toHaveBeenCalled();

		monitor.status = { enabled: true, sourceName: 'N64 Capture', recordingState: null };
		monitor.loaded = true;
		obsSources.items = [{ name: 'N64 Capture', id: 'video_capture_device' }];
		obsSources.loaded = true;

		await screen.findByRole('button', { name: /stop monitoring/i });
		expect(mocks.api.startMonitor).not.toHaveBeenCalled();
		expect(mocks.goto).not.toHaveBeenCalled();
	});

	it('waits for an inactive snapshot before starting a monitor', async () => {
		monitor.status = null;
		monitor.loaded = false;

		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		await Promise.resolve();
		expect(mocks.api.startMonitor).not.toHaveBeenCalled();

		monitor.status = { enabled: false, recordingState: null };
		monitor.loaded = true;

		await waitFor(() => expect(mocks.api.startMonitor).toHaveBeenCalledTimes(1));
	});

	it('stops a monitor when it is already started', async () => {
		const user = userEvent.setup();
		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		const stopButton = await screen.findByRole('button', { name: /stop monitoring/i });
		await user.click(stopButton);

		await waitFor(() => expect(mocks.api.stopMonitor).toHaveBeenCalledTimes(1));
		// Monitor status is now owned by backend snapshots; this page only
		// performs the stop request and navigates away while the socket update lands.
		expect(mocks.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});

	it('asks on the first stop and saves a preference to stop the replay buffer', async () => {
		const user = userEvent.setup();
		settings.applyReloaded(
			{ ...settings.defaults, stopReplayBufferPromptShown: false, welcomeModalShown: true },
			'/tmp/the-golden-eye/settings.json'
		);
		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		await user.click(await screen.findByRole('button', { name: /stop monitoring/i }));

		expect(await screen.findByRole('dialog', { name: /stop the replay buffer too/i })).toBeInTheDocument();
		expect(screen.getByText(/change this later in the plugin's Options/i)).toBeInTheDocument();
		expect(mocks.api.stopMonitor).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: /^stop replay buffer$/i }));

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(
				expect.objectContaining({
					stopReplayBufferWhenMonitorStopped: true,
					stopReplayBufferPromptShown: true
				})
			)
		);
		await waitFor(() => expect(mocks.api.stopMonitor).toHaveBeenCalledTimes(1));
	});

	it('keeps the replay buffer running when that first-stop preference is chosen', async () => {
		const user = userEvent.setup();
		settings.applyReloaded(
			{ ...settings.defaults, stopReplayBufferPromptShown: false, welcomeModalShown: true },
			'/tmp/the-golden-eye/settings.json'
		);
		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		await user.click(await screen.findByRole('button', { name: /stop monitoring/i }));
		await user.click(await screen.findByRole('button', { name: /keep it running/i }));

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(
				expect.objectContaining({
					stopReplayBufferWhenMonitorStopped: false,
					stopReplayBufferPromptShown: true
				})
			)
		);
		await waitFor(() => expect(mocks.api.stopMonitor).toHaveBeenCalledTimes(1));
	});
});
