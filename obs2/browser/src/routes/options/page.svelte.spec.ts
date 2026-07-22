import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Settings } from '$lib/stores/settings.svelte';
import OptionsPageHarness from './page.test-harness.svelte';
import { monitor } from '$lib/stores/monitor.svelte';
import { replayBuffer } from '$lib/stores/replayBuffer.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { updates } from '$lib/stores/updates.svelte';

const mocks = vi.hoisted(() => {
	const api = {
		getReplayBufferStatus: vi.fn(),
		getUpdateStatus: vi.fn(),
		checkForUpdateNow: vi.fn(),
		downloadUpdateNow: vi.fn(),
		applyUpdateNow: vi.fn(),
		putSettings: vi.fn(),
		resetSettingsToDefaults: vi.fn(),
		getYouTubeStatus: vi.fn()
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

vi.mock('$lib/stores/appSocket.svelte', () => ({
	startAppSocket: mocks.startAppSocket,
	stopAppSocket: mocks.stopAppSocket
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			getReplayBufferStatus: mocks.api.getReplayBufferStatus,
			getUpdateStatus: mocks.api.getUpdateStatus,
			checkForUpdateNow: mocks.api.checkForUpdateNow,
			downloadUpdateNow: mocks.api.downloadUpdateNow,
			applyUpdateNow: mocks.api.applyUpdateNow,
			putSettings: mocks.api.putSettings,
			resetSettingsToDefaults: mocks.api.resetSettingsToDefaults,
			getYouTubeStatus: mocks.api.getYouTubeStatus
		}
	};
});

const defaultSettings: Settings = {
	stopReplayBufferWhenMonitorStopped: false,
	showMonitorFps: false,
	showDeveloperSettings: false,
	showSourcePreviews: true,
	welcomeModalShown: true,
	completedOutputPath: '',
	saveFailedRuns: true,
	failedOutputPath: '',
	failedRunLimit: 10,
	minimumFailedRunLengthSecs: 20,
	clipFilenameTemplate: '{level} - {time} - {difficulty} - {status}',
	preRunPaddingSecs: 5,
	postRunPaddingSecs: 5,
	discordNotificationsEnabled: true,
	discordWebhookUrl: '',
	streamingStartedMessageTemplate: 'Bond is now streaming at: {broadcast_url}',
	streamingStoppedMessageTemplate: 'Bond stopped streaming at: {broadcast_url}',
	updateCheckInterval: 'weekly',
	lastUpdateCheckTime: null,
	autoUpdateEnabled: false,
	youtubeVisibility: 'unlisted',
	youtubeTitleTemplate: '{level} - {difficulty} - {time}',
	youtubeDescriptionTemplate: 'Achieved at {datetime_local}\n\nRecorded with The Golden Eye {plugin_version}.'
};

const availableReplayBuffer = {
	enabled: true,
	available: true,
	active: true,
	maxSeconds: 1200,
	outputDirectory: '/captures',
	defaultCompletedOutputPath: '/captures/GoldenEye',
	defaultFailedOutputPath: '/captures/GoldenEye - failed'
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
	settings.applyReloaded(defaultSettings, '/tmp/the-golden-eye/settings.json', defaultSettings);
	settings.loaded = true;
	updates.applyStatus({ phase: 'idle', available: null });
	mocks.api.getReplayBufferStatus.mockResolvedValue(availableReplayBuffer);
	mocks.api.putSettings.mockImplementation(async (next: Settings) => next);
	mocks.api.resetSettingsToDefaults.mockResolvedValue(defaultSettings);
	mocks.api.getUpdateStatus.mockResolvedValue({ phase: 'idle', available: null });
	mocks.api.checkForUpdateNow.mockResolvedValue({ update: null });
	mocks.api.downloadUpdateNow.mockResolvedValue(undefined);
	mocks.api.applyUpdateNow.mockResolvedValue(undefined);
	mocks.api.getYouTubeStatus.mockResolvedValue({
		enabled: true,
		oauthConfigured: true,
		connected: false,
		account: null,
		uploads: [],
		history: []
	});
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

	it('confirms before resetting settings to defaults', async () => {
		const user = userEvent.setup();
		settings.applyReloaded(
			{ ...defaultSettings, discordWebhookUrl: 'https://discord.example/secret' },
			'/tmp/the-golden-eye/settings.json',
			defaultSettings
		);
		render(OptionsPageHarness);

		await user.click(await screen.findByRole('button', { name: /^Reset to defaults$/i }));
		const dialog = screen.getByRole('dialog', { name: /Reset settings/i });
		expect(dialog).toHaveTextContent(/Discord webhook URL/i);
		expect(mocks.api.resetSettingsToDefaults).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: /^Cancel$/i }));
		expect(dialog).not.toBeInTheDocument();
		expect(mocks.api.resetSettingsToDefaults).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: /^Reset to defaults$/i }));
		await user.click(
			within(screen.getByRole('dialog', { name: /Reset settings/i })).getByRole('button', {
				name: /^Reset to defaults$/i
			})
		);

		await waitFor(() => expect(mocks.api.resetSettingsToDefaults).toHaveBeenCalledOnce());
		await waitFor(() => expect(screen.queryByRole('dialog', { name: /Reset settings/i })).not.toBeInTheDocument());
		expect(settings.discordWebhookUrl).toBe('');
	});

	it('checks, then offers an explicit download and apply when auto-install is off', async () => {
		const update = {
			currentVersion: 'v1.0.0',
			latestVersion: 'v1.1.0',
			releaseUrl: 'https://example.com/release'
		};
		mocks.api.checkForUpdateNow.mockResolvedValue({ update });
		mocks.api.getUpdateStatus
			.mockResolvedValueOnce({ phase: 'available', available: update })
			.mockResolvedValueOnce({ phase: 'staged', available: update });
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
		updates.applyStatus({ phase: 'staged', available: null });
		render(OptionsPageHarness);

		const applyButton = await screen.findByRole('button', { name: /^Apply update now$/i });
		expect(applyButton).toBeInTheDocument();
		expect(screen.queryByRole('button', { name: /^Check now$/i })).not.toBeInTheDocument();
	});

	it('disables applying while the monitor is active and explains why', async () => {
		monitor.status = { enabled: true, sourceName: 'N64 Capture', recordingState: null };
		updates.applyStatus({ phase: 'staged', available: null });
		render(OptionsPageHarness);

		const applyButton = await screen.findByRole('button', { name: /^Apply update now$/i });
		expect(applyButton).toBeDisabled();
		expect(screen.getByText("The update can't be applied while the monitor is active.")).toBeInTheDocument();
	});

	it('shows and persists the first-run welcome acknowledgement', async () => {
		const user = userEvent.setup();
		settings.applyReloaded(
			{ ...defaultSettings, welcomeModalShown: false },
			'/tmp/the-golden-eye/settings.json',
			defaultSettings
		);

		render(OptionsPageHarness);

		const dialog = await screen.findByRole('dialog', { name: /Welcome to The Golden Eye/i });
		await user.click(screen.getByRole('button', { name: /I understand/i }));

		await waitFor(() => expect(dialog).not.toBeInTheDocument());
		await waitFor(() =>
			expect(mocks.api.putSettings).toHaveBeenCalledWith(expect.objectContaining({ welcomeModalShown: true }))
		);
	});
});
