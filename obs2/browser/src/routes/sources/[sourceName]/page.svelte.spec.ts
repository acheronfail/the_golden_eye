import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import SourcePage from './+page.svelte';
import { monitor } from '$lib/monitor.svelte';
import { settings } from '$lib/settings.svelte';
import { obsSources } from '$lib/sources.svelte';

const mocks = vi.hoisted(() => {
	const api = {
		getMonitorStatus: vi.fn(),
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
		getMonitorStatus: mocks.api.getMonitorStatus,
		getReplayBufferStatus: mocks.api.getReplayBufferStatus,
		startMonitor: mocks.api.startMonitor,
		stopMonitor: mocks.api.stopMonitor
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
	settings.loaded = true;
	mocks.api.getMonitorStatus.mockResolvedValue({
		enabled: true,
		sourceName: 'N64 Capture',
		recordingState: null
	});
	mocks.api.getReplayBufferStatus.mockResolvedValue({
		enabled: true,
		available: true,
		active: true,
		maxSeconds: 1200,
		outputDirectory: '/captures',
		defaultCompletedOutputPath: '/captures/Goldeneye',
		defaultFailedOutputPath: '/captures/Goldeneye/failed'
	});
	mocks.api.startMonitor.mockResolvedValue(undefined);
	mocks.api.stopMonitor.mockResolvedValue(undefined);
});

describe('/sources/[sourceName]', () => {
	it('stops a monitor when it is already started', async () => {
		const user = userEvent.setup();
		render(SourcePage, { props: { data: {}, params: { sourceName: 'N64 Capture' } } });

		const stopButton = await screen.findByRole('button', { name: /stop monitoring/i });
		await user.click(stopButton);

		await waitFor(() => expect(mocks.api.stopMonitor).toHaveBeenCalledTimes(1));
		expect(monitor.status).toEqual({ enabled: false, recordingState: null });
		expect(mocks.goto).toHaveBeenCalledWith('/', { replaceState: true });
	});
});
