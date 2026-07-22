import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SourcePage from './+page.svelte';
import { monitor } from '$lib/stores/monitor.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { obsSources } from '$lib/stores/sources.svelte';

const mocks = vi.hoisted(() => {
	const api = {
		getReplayBufferStatus: vi.fn(),
		startMonitor: vi.fn(),
		stopMonitor: vi.fn()
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
			getReplayBufferStatus: mocks.api.getReplayBufferStatus,
			startMonitor: mocks.api.startMonitor,
			stopMonitor: mocks.api.stopMonitor
		}
	};
});

beforeEach(() => {
	vi.clearAllMocks();
	mocks.page.url = new URL('http://localhost/sources/N64%20Capture');
	obsSources.items = [{ name: 'N64 Capture', id: 'video_capture_device' }];
	obsSources.loaded = true;
	monitor.status = {
		enabled: true,
		sourceName: 'N64 Capture',
		recordingState: null
	};
	monitor.loaded = true;
	monitor.match = null;
	monitor.recordingState = null;
	monitor.chromePhase = null;
	settings.loaded = true;
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
});

describe('/sources/[sourceName]', () => {
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
});
