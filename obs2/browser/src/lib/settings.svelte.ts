import { browser } from '$app/environment';
import { z } from 'zod';
import {
	getSettingsStatus,
	putSettings,
	resetSettingsToDefaults,
	revealSettingsConfig,
	type SettingsStatus
} from './api';

export const DEFAULT_CLIP_FILENAME_TEMPLATE = '{level}/{difficulty}/{time} - {timestamp_local}';
export const DEFAULT_PRE_RUN_PADDING_SECS = 5;
export const DEFAULT_POST_RUN_PADDING_SECS = 5;
export const DEFAULT_MIN_FAILED_RUN_LEN_SECS = 10;
export const DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE = '\u{1f7e2} Bond is now streaming at: {broadcast_url}';
export const DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE =
	'\u{1f534} Bond stopped streaming at <t:{unix_seconds}:F>: {broadcast_url}';
const LEGACY_CLIP_FILENAME_TEMPLATE = '{replay} - clip - {level}{time_suffix}{failed_suffix}';

const SettingsSchema = z.object({
	stopReplayBufferWhenMonitorStopped: z.boolean().catch(false),
	showMonitorFps: z.boolean().catch(false),
	completedOutputPath: z.string().catch(''),
	saveFailedRuns: z.boolean().catch(true),
	failedOutputPath: z.string().catch(''),
	failedRunLimit: z.coerce.number().int().min(0).catch(0),
	minimumFailedRunLengthSecs: z.coerce.number().min(0).catch(DEFAULT_MIN_FAILED_RUN_LEN_SECS),
	clipFilenameTemplate: z.string().catch(DEFAULT_CLIP_FILENAME_TEMPLATE),
	preRunPaddingSecs: z.coerce.number().min(0).catch(DEFAULT_PRE_RUN_PADDING_SECS),
	postRunPaddingSecs: z.coerce.number().min(0).catch(DEFAULT_POST_RUN_PADDING_SECS),
	discordNotificationsEnabled: z.boolean().catch(true),
	discordWebhookUrl: z.string().catch(''),
	streamingStartedMessageTemplate: z.string().catch(DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE),
	streamingStoppedMessageTemplate: z.string().catch(DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE)
});
export type Settings = z.infer<typeof SettingsSchema>;

export interface RecordingOptions {
	completedOutputPath: string;
	saveFailedRuns: boolean;
	failedOutputPath: string;
	failedRunLimit: number;
	minimumFailedRunLengthSecs: number;
	clipFilenameTemplate: string;
	preRunPaddingSecs: number;
	postRunPaddingSecs: number;
}

const nonNegativeInt = (value: unknown): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : 0;
};

const nonNegativeNumber = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, n) : fallback;
};

const normalizeClipFilenameTemplate = (value: string | undefined): string => {
	if (!value || value === LEGACY_CLIP_FILENAME_TEMPLATE) return DEFAULT_CLIP_FILENAME_TEMPLATE;
	return value;
};

const normalizeMessageTemplate = (value: string | undefined, fallback: string): string => {
	if (!value || value.trim() === '') return fallback;
	return value;
};

const parseSettings = (value: unknown): Settings => {
	const parsed = SettingsSchema.parse(value);
	return {
		...parsed,
		failedRunLimit: nonNegativeInt(parsed.failedRunLimit),
		minimumFailedRunLengthSecs: nonNegativeNumber(parsed.minimumFailedRunLengthSecs, DEFAULT_MIN_FAILED_RUN_LEN_SECS),
		clipFilenameTemplate: normalizeClipFilenameTemplate(parsed.clipFilenameTemplate),
		preRunPaddingSecs: nonNegativeNumber(parsed.preRunPaddingSecs, DEFAULT_PRE_RUN_PADDING_SECS),
		postRunPaddingSecs: nonNegativeNumber(parsed.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS),
		streamingStartedMessageTemplate: normalizeMessageTemplate(
			parsed.streamingStartedMessageTemplate,
			DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE
		),
		streamingStoppedMessageTemplate: normalizeMessageTemplate(
			parsed.streamingStoppedMessageTemplate,
			DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE
		)
	};
};

const defaultSettings = (): Settings => parseSettings({});
const serializeSettings = (settings: Settings): string => JSON.stringify(settings);
const initialSettings = defaultSettings();
const initialSavedState = serializeSettings(initialSettings);

const errorMessage = (err: unknown): string => (err instanceof Error ? err.message : String(err));

export const settings = new (class {
	//
	// Load/save state
	//

	loaded = $state(!browser);
	loading = $state(false);
	saving = $state(false);
	saveError = $state<string | null>(null);
	configPath = $state('');
	fileError = $state<string | null>(null);
	lastSavedState = $state(initialSavedState);

	private loadPromise: Promise<void> | null = null;
	private savePromise: Promise<void> | null = null;
	private saveQueued = false;

	//
	// General
	//

	stopReplayBufferWhenMonitorStopped = $state(initialSettings.stopReplayBufferWhenMonitorStopped);
	showMonitorFps = $state(initialSettings.showMonitorFps);

	//
	// Recording
	//

	completedOutputPath = $state(initialSettings.completedOutputPath);
	saveFailedRuns = $state(initialSettings.saveFailedRuns);
	failedOutputPath = $state(initialSettings.failedOutputPath);
	failedRunLimit = $state(initialSettings.failedRunLimit);
	minimumFailedRunLengthSecs = $state(initialSettings.minimumFailedRunLengthSecs);
	clipFilenameTemplate = $state(initialSettings.clipFilenameTemplate);
	preRunPaddingSecs = $state(initialSettings.preRunPaddingSecs);
	postRunPaddingSecs = $state(initialSettings.postRunPaddingSecs);

	//
	// Notifications
	//

	discordNotificationsEnabled = $state(initialSettings.discordNotificationsEnabled);
	discordWebhookUrl = $state(initialSettings.discordWebhookUrl);
	streamingStartedMessageTemplate = $state(initialSettings.streamingStartedMessageTemplate);
	streamingStoppedMessageTemplate = $state(initialSettings.streamingStoppedMessageTemplate);

	recordingOptions: RecordingOptions = $derived({
		completedOutputPath: this.completedOutputPath.trim(),
		saveFailedRuns: this.saveFailedRuns,
		failedOutputPath: this.failedOutputPath.trim(),
		failedRunLimit: nonNegativeInt(this.failedRunLimit),
		minimumFailedRunLengthSecs: nonNegativeNumber(this.minimumFailedRunLengthSecs, DEFAULT_MIN_FAILED_RUN_LEN_SECS),
		clipFilenameTemplate: this.clipFilenameTemplate.trim() || DEFAULT_CLIP_FILENAME_TEMPLATE,
		preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs, DEFAULT_PRE_RUN_PADDING_SECS),
		postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS)
	});

	//
	// Stored
	//

	savedState = $derived(
		JSON.stringify({
			stopReplayBufferWhenMonitorStopped: this.stopReplayBufferWhenMonitorStopped,
			showMonitorFps: this.showMonitorFps,
			completedOutputPath: this.completedOutputPath,
			saveFailedRuns: this.saveFailedRuns,
			failedOutputPath: this.failedOutputPath,
			failedRunLimit: nonNegativeInt(this.failedRunLimit),
			minimumFailedRunLengthSecs: nonNegativeNumber(this.minimumFailedRunLengthSecs, DEFAULT_MIN_FAILED_RUN_LEN_SECS),
			clipFilenameTemplate: this.clipFilenameTemplate,
			preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs, DEFAULT_PRE_RUN_PADDING_SECS),
			postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS),
			discordNotificationsEnabled: this.discordNotificationsEnabled,
			discordWebhookUrl: this.discordWebhookUrl,
			streamingStartedMessageTemplate: this.streamingStartedMessageTemplate,
			streamingStoppedMessageTemplate: this.streamingStoppedMessageTemplate
		})
	);

	dirty = $derived(this.savedState !== this.lastSavedState);
	canEdit = $derived(this.loaded && this.fileError === null);

	async load(): Promise<void> {
		if (!browser) return;
		if (this.loadPromise) return this.loadPromise;

		this.loading = true;
		this.saveError = null;
		this.loadPromise = (async () => {
			try {
				const status = await getSettingsStatus();
				const remote = parseSettings(status.settings);

				this.configPath = status.configPath;
				this.fileError = status.fileError ?? null;

				if (this.dirty && this.fileError === null) {
					this.loaded = true;
					await this.saveNow();
				} else {
					this.apply(remote);
					this.lastSavedState = this.savedState;
					this.loaded = true;
				}
			} catch (err) {
				this.saveError = errorMessage(err);
				throw err;
			} finally {
				this.loading = false;
				this.loadPromise = null;
			}
		})();

		return this.loadPromise;
	}

	saveImmediately(): void {
		void this.saveNow().catch((err) => {
			console.warn('Failed to save settings', err);
		});
	}

	async saveNow(): Promise<void> {
		if (!browser) return;
		if (!this.loaded) await this.load();
		if (this.fileError !== null) return;
		if (!this.dirty) return;

		if (this.savePromise) {
			this.saveQueued = true;
			await this.savePromise;
			if (this.dirty) await this.saveNow();
			return;
		}

		const snapshot = this.snapshot();
		const snapshotState = serializeSettings(snapshot);
		if (snapshotState === this.lastSavedState) return;

		this.saving = true;
		this.saveError = null;
		this.savePromise = (async () => {
			try {
				const saved = parseSettings(await putSettings(snapshot));
				const savedState = serializeSettings(saved);
				this.fileError = null;

				if (this.savedState === snapshotState) {
					this.apply(saved);
					this.lastSavedState = this.savedState;
				} else {
					this.lastSavedState = savedState;
					this.saveQueued = true;
				}
			} catch (err) {
				this.saveError = errorMessage(err);
				throw err;
			} finally {
				this.saving = false;
				this.savePromise = null;
			}
		})();

		await this.savePromise;

		if (this.saveQueued) {
			this.saveQueued = false;
			if (this.dirty) await this.saveNow();
		}
	}

	applyReloaded(next: Settings, configPath: string): void {
		this.configPath = configPath;
		this.fileError = null;
		this.apply(parseSettings(next));
		this.lastSavedState = this.savedState;
		this.loaded = true;
		this.saveError = null;
	}

	applyInvalid(error: string, configPath: string): void {
		this.configPath = configPath;
		this.fileError = error;
		this.loaded = true;
	}

	async resetToDefaults(): Promise<void> {
		const reset = parseSettings(await resetSettingsToDefaults());
		this.apply(reset);
		this.lastSavedState = this.savedState;
		this.fileError = null;
		this.loaded = true;
		this.saveError = null;
	}

	async revealConfigFile(): Promise<void> {
		await revealSettingsConfig();
		const status: SettingsStatus = await getSettingsStatus();
		this.configPath = status.configPath;
		this.fileError = status.fileError ?? null;
	}

	private snapshot(): Settings {
		return parseSettings(JSON.parse(this.savedState));
	}

	private apply(next: Settings): void {
		this.stopReplayBufferWhenMonitorStopped = next.stopReplayBufferWhenMonitorStopped;
		this.showMonitorFps = next.showMonitorFps;
		this.completedOutputPath = next.completedOutputPath;
		this.saveFailedRuns = next.saveFailedRuns;
		this.failedOutputPath = next.failedOutputPath;
		this.failedRunLimit = next.failedRunLimit;
		this.minimumFailedRunLengthSecs = next.minimumFailedRunLengthSecs;
		this.clipFilenameTemplate = normalizeClipFilenameTemplate(next.clipFilenameTemplate);
		this.preRunPaddingSecs = next.preRunPaddingSecs;
		this.postRunPaddingSecs = next.postRunPaddingSecs;
		this.discordNotificationsEnabled = next.discordNotificationsEnabled;
		this.discordWebhookUrl = next.discordWebhookUrl;
		this.streamingStartedMessageTemplate = normalizeMessageTemplate(
			next.streamingStartedMessageTemplate,
			DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE
		);
		this.streamingStoppedMessageTemplate = normalizeMessageTemplate(
			next.streamingStoppedMessageTemplate,
			DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE
		);
	}
})();
