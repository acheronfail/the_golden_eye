import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '$lib/settings.svelte';
import OptionsPageHarness from './page.test-harness.svelte';
import { monitor } from '$lib/monitor.svelte';
import { replayBuffer } from '$lib/replayBuffer.svelte';
import { settings } from '$lib/settings.svelte';

const mocks = vi.hoisted(() => {
	const api = {
		getMonitorStatus: vi.fn(),
		getReplayBufferStatus: vi.fn(),
		getSettingsStatus: vi.fn(),
		getUpdateStatus: vi.fn(),
		checkForUpdateNow: vi.fn(),
		downloadUpdateNow: vi.fn(),
		applyUpdateNow: vi.fn(),
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
		page: { url: new URL('http://localhost/options') },
		startAppSocket: vi.fn(),
		stopAppSocket: vi.fn()
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

vi.mock('$lib/appSocket.svelte', () => ({
	startAppSocket: mocks.startAppSocket,
	stopAppSocket: mocks.stopAppSocket
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		getMonitorStatus: mocks.api.getMonitorStatus,
		getReplayBufferStatus: mocks.api.getReplayBufferStatus,
		getSettingsStatus: mocks.api.getSettingsStatus,
		getUpdateStatus: mocks.api.getUpdateStatus,
		checkForUpdateNow: mocks.api.checkForUpdateNow,
		downloadUpdateNow: mocks.api.downloadUpdateNow,
		applyUpdateNow: mocks.api.applyUpdateNow,
		putSettings: mocks.api.putSettings
	};
});

const defaultSettings: Settings = {
	stopReplayBufferWhenMonitorStopped: false,
	showMonitorFps: false,
	showDeveloperSettings: false,
	welcomeModalShown: true,
	completedOutputPath: '',
	saveFailedRuns: true,
	failedOutputPath: '',
	failedRunLimit: 0,
	minimumFailedRunLengthSecs: 10,
	clipFilenameTemplate: '{level} - {time} - {difficulty} - {status}',
	preRunPaddingSecs: 5,
	postRunPaddingSecs: 5,
	discordNotificationsEnabled: true,
	discordWebhookUrl: '',
	streamingStartedMessageTemplate: 'Bond is now streaming at: {broadcast_url}',
	streamingStoppedMessageTemplate: 'Bond stopped streaming at: {broadcast_url}',
	updateCheckInterval: 'weekly',
	lastUpdateCheckTime: null,
	autoUpdateEnabled: false
};

const availableReplayBuffer = {
	enabled: true,
	available: true,
	active: true,
	maxSeconds: 1200,
	outputDirectory: '/captures',
	defaultCompletedOutputPath: '/captures/Goldeneye',
	defaultFailedOutputPath: '/captures/Goldeneye/failed'
};

beforeEach(() => {
	vi.clearAllMocks();
	mocks.page.url = new URL('http://localhost/options');
	replayBuffer.status = availableReplayBuffer;
	replayBuffer.loaded = true;
	monitor.status = { enabled: false, recordingState: null };
	monitor.loaded = true;
	monitor.match = null;
	monitor.recordingState = null;
	settings.applyReloaded(defaultSettings, '/tmp/the-golden-eye/settings.json');
	settings.loaded = true;
	mocks.api.getMonitorStatus.mockResolvedValue({ enabled: false, recordingState: null });
	mocks.api.getReplayBufferStatus.mockResolvedValue(availableReplayBuffer);
	mocks.api.getSettingsStatus.mockResolvedValue({
		settings: defaultSettings,
		configPath: '/tmp/the-golden-eye/settings.json',
		fileError: null
	});
	mocks.api.putSettings.mockImplementation(async (next: Settings) => next);
	mocks.api.getUpdateStatus.mockResolvedValue({ staged: false });
	mocks.api.checkForUpdateNow.mockResolvedValue({ update: null });
	mocks.api.downloadUpdateNow.mockResolvedValue(undefined);
	mocks.api.applyUpdateNow.mockResolvedValue(undefined);
});

describe('/options', () => {
	it('saves to the backend after updating an option', async () => {
		const user = userEvent.setup();
		render(OptionsPageHarness);

		const checkbox = await screen.findByRole('checkbox', { name: /Stop replay buffer when monitor stopped/i });
		await waitFor(() => expect(checkbox).toBeEnabled());
		await user.click(checkbox);

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(
				expect.objectContaining({ stopReplayBufferWhenMonitorStopped: true })
			)
		);
	});

	it('saves the monitor FPS display option', async () => {
		const user = userEvent.setup();
		render(OptionsPageHarness);

		const checkbox = await screen.findByRole('checkbox', { name: /Show monitor FPS/i });
		await waitFor(() => expect(checkbox).toBeEnabled());
		await user.click(checkbox);

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(expect.objectContaining({ showMonitorFps: true }))
		);
	});

	it('saves the developer settings visibility option', async () => {
		const user = userEvent.setup();
		render(OptionsPageHarness);

		const checkbox = await screen.findByRole('checkbox', { name: /Show developer settings/i });
		await waitFor(() => expect(checkbox).toBeEnabled());
		await user.click(checkbox);

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(expect.objectContaining({ showDeveloperSettings: true }))
		);
	});

	it('saves the plugin update check interval', async () => {
		const user = userEvent.setup();
		render(OptionsPageHarness);

		const select = await screen.findByRole('combobox', { name: /Check for plugin updates/i });
		await waitFor(() => expect(select).toBeEnabled());
		await user.click(select);
		await user.click(await screen.findByRole('option', { name: /^Daily$/i }));

		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(expect.objectContaining({ updateCheckInterval: 'daily' }))
		);
	});

	it('checks, then offers an explicit download and apply when auto-install is off', async () => {
		mocks.api.checkForUpdateNow.mockResolvedValue({
			update: { currentVersion: 'v1.0.0', latestVersion: 'v1.1.0', releaseUrl: 'https://example.com/release' }
		});
		const user = userEvent.setup();
		render(OptionsPageHarness);

		const checkButton = await screen.findByRole('button', { name: /^Check now$/i });
		await waitFor(() => expect(checkButton).toBeEnabled());
		await user.click(checkButton);

		// Auto-install is off, so a found update surfaces an explicit "Download
		// now" rather than downloading on its own.
		const downloadButton = await screen.findByRole('button', { name: /^Download now$/i });
		expect(mocks.api.checkForUpdateNow).toHaveBeenCalled();
		expect(mocks.api.downloadUpdateNow).not.toHaveBeenCalled();

		await user.click(downloadButton);

		// Once the download finishes staging, the button becomes "Apply update now".
		await screen.findByRole('button', { name: /^Apply update now$/i });
		expect(mocks.api.downloadUpdateNow).toHaveBeenCalled();
	});

	it('shows "Apply update now" when an update is already staged', async () => {
		mocks.api.getUpdateStatus.mockResolvedValue({ staged: true });
		render(OptionsPageHarness);

		const applyButton = await screen.findByRole('button', { name: /^Apply update now$/i });
		expect(applyButton).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /^Check now$/i })).not.toBeInTheDocument();
	});

	it('shows and persists the first-run welcome acknowledgement', async () => {
		const user = userEvent.setup();
		mocks.api.getSettingsStatus.mockResolvedValue({
			settings: { ...defaultSettings, welcomeModalShown: false },
			configPath: '/tmp/the-golden-eye/settings.json',
			fileError: null
		});

		render(OptionsPageHarness);

		const dialog = await screen.findByRole('dialog', { name: /Welcome to The Golden Eye/i });
		await user.click(screen.getByRole('button', { name: /I understand/i }));

		await waitFor(() => expect(dialog).not.toBeInTheDocument());
		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(expect.objectContaining({ welcomeModalShown: true }))
		);
	});
});
