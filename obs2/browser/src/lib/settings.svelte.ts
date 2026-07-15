import { browser } from '$app/environment';
import { z } from 'zod';
import {
	getSettingsStatus,
	putSettings,
	resetSettingsToDefaults,
	revealSettingsConfig,
	type SettingsStatus
} from './api';

const LEGACY_CLIP_FILENAME_TEMPLATE = '{replay} - clip - {level}{time_suffix}{failed_suffix}';
const UpdateCheckIntervalSchema = z.enum(['monthly', 'weekly', 'daily', 'never']);
export type UpdateCheckInterval = z.infer<typeof UpdateCheckIntervalSchema>;

export interface Settings {
	stopReplayBufferWhenMonitorStopped: boolean;
	showMonitorFps: boolean;
	showDeveloperSettings: boolean;
	welcomeModalShown: boolean;
	completedOutputPath: string;
	saveFailedRuns: boolean;
	failedOutputPath: string;
	failedRunLimit: number;
	minimumFailedRunLengthSecs: number;
	clipFilenameTemplate: string;
	preRunPaddingSecs: number;
	postRunPaddingSecs: number;
	discordNotificationsEnabled: boolean;
	discordWebhookUrl: string;
	streamingStartedMessageTemplate: string;
	streamingStoppedMessageTemplate: string;
	updateCheckInterval: UpdateCheckInterval;
	lastUpdateCheckTime: number | null;
	autoUpdateEnabled: boolean;
}

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

const nonNegativeInt = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : fallback;
};

const nonNegativeNumber = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, n) : fallback;
};

const bootstrapSettings: Settings = {
	stopReplayBufferWhenMonitorStopped: false,
	showMonitorFps: false,
	showDeveloperSettings: false,
	welcomeModalShown: false,
	completedOutputPath: '',
	saveFailedRuns: true,
	failedOutputPath: '',
	failedRunLimit: 0,
	minimumFailedRunLengthSecs: 0,
	clipFilenameTemplate: '',
	preRunPaddingSecs: 0,
	postRunPaddingSecs: 0,
	discordNotificationsEnabled: true,
	discordWebhookUrl: '',
	streamingStartedMessageTemplate: '',
	streamingStoppedMessageTemplate: '',
	updateCheckInterval: 'weekly',
	lastUpdateCheckTime: null,
	autoUpdateEnabled: false
};

const settingsSchema = (defaults: Settings) =>
	z.object({
		stopReplayBufferWhenMonitorStopped: z.boolean().catch(defaults.stopReplayBufferWhenMonitorStopped),
		showMonitorFps: z.boolean().catch(defaults.showMonitorFps),
		showDeveloperSettings: z.boolean().catch(defaults.showDeveloperSettings),
		welcomeModalShown: z.boolean().catch(defaults.welcomeModalShown),
		completedOutputPath: z.string().catch(defaults.completedOutputPath),
		saveFailedRuns: z.boolean().catch(defaults.saveFailedRuns),
		failedOutputPath: z.string().catch(defaults.failedOutputPath),
		failedRunLimit: z.coerce.number().int().min(0).catch(defaults.failedRunLimit),
		minimumFailedRunLengthSecs: z.coerce.number().min(0).catch(defaults.minimumFailedRunLengthSecs),
		clipFilenameTemplate: z.string().catch(defaults.clipFilenameTemplate),
		preRunPaddingSecs: z.coerce.number().min(0).catch(defaults.preRunPaddingSecs),
		postRunPaddingSecs: z.coerce.number().min(0).catch(defaults.postRunPaddingSecs),
		discordNotificationsEnabled: z.boolean().catch(defaults.discordNotificationsEnabled),
		discordWebhookUrl: z.string().catch(defaults.discordWebhookUrl),
		streamingStartedMessageTemplate: z.string().catch(defaults.streamingStartedMessageTemplate),
		streamingStoppedMessageTemplate: z.string().catch(defaults.streamingStoppedMessageTemplate),
		updateCheckInterval: UpdateCheckIntervalSchema.catch(defaults.updateCheckInterval),
		lastUpdateCheckTime: z.coerce.number().int().min(0).nullable().catch(defaults.lastUpdateCheckTime),
		autoUpdateEnabled: z.boolean().catch(defaults.autoUpdateEnabled)
	});

const normalizeClipFilenameTemplate = (value: string | undefined, fallback: string): string => {
	if (!value || value === LEGACY_CLIP_FILENAME_TEMPLATE) return fallback;
	return value;
};

const normalizeMessageTemplate = (value: string | undefined, fallback: string): string => {
	if (!value || value.trim() === '') return fallback;
	return value;
};

const parseSettings = (value: unknown, defaults = bootstrapSettings): Settings => {
	const parsed = settingsSchema(defaults).parse(value);
	return {
		...parsed,
		failedRunLimit: nonNegativeInt(parsed.failedRunLimit, defaults.failedRunLimit),
		minimumFailedRunLengthSecs: nonNegativeNumber(
			parsed.minimumFailedRunLengthSecs,
			defaults.minimumFailedRunLengthSecs
		),
		clipFilenameTemplate: normalizeClipFilenameTemplate(parsed.clipFilenameTemplate, defaults.clipFilenameTemplate),
		preRunPaddingSecs: nonNegativeNumber(parsed.preRunPaddingSecs, defaults.preRunPaddingSecs),
		postRunPaddingSecs: nonNegativeNumber(parsed.postRunPaddingSecs, defaults.postRunPaddingSecs),
		streamingStartedMessageTemplate: normalizeMessageTemplate(
			parsed.streamingStartedMessageTemplate,
			defaults.streamingStartedMessageTemplate
		),
		streamingStoppedMessageTemplate: normalizeMessageTemplate(
			parsed.streamingStoppedMessageTemplate,
			defaults.streamingStoppedMessageTemplate
		)
	};
};

const serializeSettings = (settings: Settings): string => JSON.stringify(settings);
const initialSettings = parseSettings(bootstrapSettings);
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
	pluginVersion = $state('0.0.0-unknown');
	lastSavedState = $state(initialSavedState);
	defaults = $state(initialSettings);

	private loadPromise: Promise<void> | null = null;
	private savePromise: Promise<void> | null = null;
	private saveQueued = false;

	//
	// General
	//

	stopReplayBufferWhenMonitorStopped = $state(initialSettings.stopReplayBufferWhenMonitorStopped);
	showMonitorFps = $state(initialSettings.showMonitorFps);
	showDeveloperSettings = $state(initialSettings.showDeveloperSettings);
	welcomeModalShown = $state(initialSettings.welcomeModalShown);
	updateCheckInterval = $state<UpdateCheckInterval>(initialSettings.updateCheckInterval);
	lastUpdateCheckTime = $state<number | null>(initialSettings.lastUpdateCheckTime);
	autoUpdateEnabled = $state(initialSettings.autoUpdateEnabled);

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
		minimumFailedRunLengthSecs: nonNegativeNumber(
			this.minimumFailedRunLengthSecs,
			this.defaults.minimumFailedRunLengthSecs
		),
		clipFilenameTemplate: this.clipFilenameTemplate.trim() || this.defaults.clipFilenameTemplate,
		preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs, this.defaults.preRunPaddingSecs),
		postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, this.defaults.postRunPaddingSecs)
	});

	//
	// Stored
	//

	savedState = $derived(
		JSON.stringify({
			stopReplayBufferWhenMonitorStopped: this.stopReplayBufferWhenMonitorStopped,
			showMonitorFps: this.showMonitorFps,
			showDeveloperSettings: this.showDeveloperSettings,
			welcomeModalShown: this.welcomeModalShown,
			completedOutputPath: this.completedOutputPath,
			saveFailedRuns: this.saveFailedRuns,
			failedOutputPath: this.failedOutputPath,
			failedRunLimit: nonNegativeInt(this.failedRunLimit),
			minimumFailedRunLengthSecs: nonNegativeNumber(
				this.minimumFailedRunLengthSecs,
				this.defaults.minimumFailedRunLengthSecs
			),
			clipFilenameTemplate: this.clipFilenameTemplate,
			preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs, this.defaults.preRunPaddingSecs),
			postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, this.defaults.postRunPaddingSecs),
			discordNotificationsEnabled: this.discordNotificationsEnabled,
			discordWebhookUrl: this.discordWebhookUrl,
			streamingStartedMessageTemplate: this.streamingStartedMessageTemplate,
			streamingStoppedMessageTemplate: this.streamingStoppedMessageTemplate,
			updateCheckInterval: this.updateCheckInterval,
			lastUpdateCheckTime: this.lastUpdateCheckTime,
			autoUpdateEnabled: this.autoUpdateEnabled
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
				this.defaults = parseSettings(status.defaults);
				const remote = parseSettings(status.settings, this.defaults);

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
				const saved = parseSettings(await putSettings(snapshot), this.defaults);
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

	applyStatus(status: SettingsStatus): void {
		this.configPath = status.configPath;
		this.fileError = status.fileError ?? null;
		this.pluginVersion = status.pluginVersion;
		this.defaults = parseSettings(status.defaults);
		this.apply(parseSettings(status.settings, this.defaults));
		this.lastSavedState = this.savedState;
		this.loaded = true;
		this.saveError = null;
	}

	applyReloaded(next: Settings, configPath: string, defaults = this.defaults): void {
		this.configPath = configPath;
		this.fileError = null;
		this.defaults = parseSettings(defaults);
		this.apply(parseSettings(next, this.defaults));
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
		const reset = parseSettings(await resetSettingsToDefaults(), this.defaults);
		this.apply(reset);
		this.lastSavedState = this.savedState;
		this.fileError = null;
		this.loaded = true;
		this.saveError = null;
	}

	async revealConfigFile(): Promise<void> {
		await revealSettingsConfig();
		const status: SettingsStatus = await getSettingsStatus();
		this.defaults = parseSettings(status.defaults);
		this.configPath = status.configPath;
		this.fileError = status.fileError ?? null;
		this.pluginVersion = status.pluginVersion;
	}

	private snapshot(): Settings {
		return parseSettings(JSON.parse(this.savedState), this.defaults);
	}

	private apply(next: Settings): void {
		this.stopReplayBufferWhenMonitorStopped = next.stopReplayBufferWhenMonitorStopped;
		this.showMonitorFps = next.showMonitorFps;
		this.showDeveloperSettings = next.showDeveloperSettings;
		this.welcomeModalShown = next.welcomeModalShown;
		this.updateCheckInterval = next.updateCheckInterval;
		this.lastUpdateCheckTime = next.lastUpdateCheckTime;
		this.autoUpdateEnabled = next.autoUpdateEnabled;
		this.completedOutputPath = next.completedOutputPath;
		this.saveFailedRuns = next.saveFailedRuns;
		this.failedOutputPath = next.failedOutputPath;
		this.failedRunLimit = next.failedRunLimit;
		this.minimumFailedRunLengthSecs = next.minimumFailedRunLengthSecs;
		this.clipFilenameTemplate = normalizeClipFilenameTemplate(
			next.clipFilenameTemplate,
			this.defaults.clipFilenameTemplate
		);
		this.preRunPaddingSecs = next.preRunPaddingSecs;
		this.postRunPaddingSecs = next.postRunPaddingSecs;
		this.discordNotificationsEnabled = next.discordNotificationsEnabled;
		this.discordWebhookUrl = next.discordWebhookUrl;
		this.streamingStartedMessageTemplate = normalizeMessageTemplate(
			next.streamingStartedMessageTemplate,
			this.defaults.streamingStartedMessageTemplate
		);
		this.streamingStoppedMessageTemplate = normalizeMessageTemplate(
			next.streamingStoppedMessageTemplate,
			this.defaults.streamingStoppedMessageTemplate
		);
	}
})();
