import { browser } from '$app/environment';
import { z } from 'zod';
import { backend, type SettingsStatus } from '$lib/api';
import type { MonitorDesign } from '$lib/components/monitorView';

const UpdateCheckIntervalSchema = z.enum(['monthly', 'weekly', 'daily', 'never']);
export type UpdateCheckInterval = z.infer<typeof UpdateCheckIntervalSchema>;
const YoutubeVisibilitySchema = z.enum(['public', 'unlisted', 'private']);
export type YoutubeVisibility = z.infer<typeof YoutubeVisibilitySchema>;
const MonitorDesignSchema = z.enum(['signal-band', 'mission-glass', 'debug']);

export interface Settings {
	stopReplayBufferWhenMonitorStopped: boolean;
	monitorDesign: MonitorDesign;
	showMonitorFps: boolean;
	showDeveloperSettings: boolean;
	showSourcePreviews: boolean;
	lastUsedSourceName: string | null;
	welcomeModalShown: boolean;
	completedOutputPath: string;
	recentRunLimit: number;
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
	youtubeVisibility: YoutubeVisibility;
	youtubeTitleTemplate: string;
	youtubeDescriptionTemplate: string;
}

export interface RecordingOptions {
	completedOutputPath: string;
	recentRunLimit: number;
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
	monitorDesign: 'signal-band',
	showMonitorFps: false,
	showDeveloperSettings: false,
	showSourcePreviews: true,
	lastUsedSourceName: null,
	welcomeModalShown: false,
	completedOutputPath: '',
	recentRunLimit: 5,
	clipFilenameTemplate: '',
	preRunPaddingSecs: 0,
	postRunPaddingSecs: 0,
	discordNotificationsEnabled: true,
	discordWebhookUrl: '',
	streamingStartedMessageTemplate: '',
	streamingStoppedMessageTemplate: '',
	updateCheckInterval: 'weekly',
	lastUpdateCheckTime: null,
	autoUpdateEnabled: false,
	youtubeVisibility: 'unlisted',
	youtubeTitleTemplate: '{level} - {difficulty} - {time}',
	youtubeDescriptionTemplate: 'Achieved at {datetime_local}\n\nRecorded with The Golden Eye {plugin_version}.'
};

const settingsSchema = (defaults: Settings) =>
	z.object({
		stopReplayBufferWhenMonitorStopped: z.boolean().catch(defaults.stopReplayBufferWhenMonitorStopped),
		monitorDesign: MonitorDesignSchema.catch(defaults.monitorDesign),
		showMonitorFps: z.boolean().catch(defaults.showMonitorFps),
		showDeveloperSettings: z.boolean().catch(defaults.showDeveloperSettings),
		showSourcePreviews: z.boolean().catch(defaults.showSourcePreviews),
		lastUsedSourceName: z.string().nullable().catch(defaults.lastUsedSourceName),
		welcomeModalShown: z.boolean().catch(defaults.welcomeModalShown),
		completedOutputPath: z.string().catch(defaults.completedOutputPath),
		recentRunLimit: z.coerce.number().int().min(1).max(20).catch(defaults.recentRunLimit),
		clipFilenameTemplate: z.string().catch(defaults.clipFilenameTemplate),
		preRunPaddingSecs: z.coerce.number().min(0).catch(defaults.preRunPaddingSecs),
		postRunPaddingSecs: z.coerce.number().min(0).catch(defaults.postRunPaddingSecs),
		discordNotificationsEnabled: z.boolean().catch(defaults.discordNotificationsEnabled),
		discordWebhookUrl: z.string().catch(defaults.discordWebhookUrl),
		streamingStartedMessageTemplate: z.string().catch(defaults.streamingStartedMessageTemplate),
		streamingStoppedMessageTemplate: z.string().catch(defaults.streamingStoppedMessageTemplate),
		updateCheckInterval: UpdateCheckIntervalSchema.catch(defaults.updateCheckInterval),
		lastUpdateCheckTime: z.coerce.number().int().min(0).nullable().catch(defaults.lastUpdateCheckTime),
		autoUpdateEnabled: z.boolean().catch(defaults.autoUpdateEnabled),
		youtubeVisibility: YoutubeVisibilitySchema.catch(defaults.youtubeVisibility),
		youtubeTitleTemplate: z.string().catch(defaults.youtubeTitleTemplate),
		youtubeDescriptionTemplate: z.string().catch(defaults.youtubeDescriptionTemplate)
	});

const normalizeClipFilenameTemplate = (value: string | undefined, fallback: string): string => {
	if (!value) return fallback;
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
		recentRunLimit: Math.min(20, Math.max(1, nonNegativeInt(parsed.recentRunLimit, defaults.recentRunLimit))),
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
	saving = $state(false);
	saveError = $state<string | null>(null);
	configPath = $state('');
	fileError = $state<string | null>(null);
	pluginVersion = $state('0.0.0-unknown');
	lastSavedState = $state(initialSavedState);
	defaults = $state(initialSettings);

	private savePromise: Promise<void> | null = null;
	private saveQueued = false;

	//
	// General
	//

	stopReplayBufferWhenMonitorStopped = $state(initialSettings.stopReplayBufferWhenMonitorStopped);
	monitorDesign = $state<MonitorDesign>(initialSettings.monitorDesign);
	showMonitorFps = $state(initialSettings.showMonitorFps);
	showDeveloperSettings = $state(initialSettings.showDeveloperSettings);
	showSourcePreviews = $state(initialSettings.showSourcePreviews);
	lastUsedSourceName = $state<string | null>(initialSettings.lastUsedSourceName);
	welcomeModalShown = $state(initialSettings.welcomeModalShown);
	updateCheckInterval = $state<UpdateCheckInterval>(initialSettings.updateCheckInterval);
	lastUpdateCheckTime = $state<number | null>(initialSettings.lastUpdateCheckTime);
	autoUpdateEnabled = $state(initialSettings.autoUpdateEnabled);

	//
	// YouTube
	//

	youtubeVisibility = $state<YoutubeVisibility>(initialSettings.youtubeVisibility);
	youtubeTitleTemplate = $state(initialSettings.youtubeTitleTemplate);
	youtubeDescriptionTemplate = $state(initialSettings.youtubeDescriptionTemplate);

	//
	// Recording
	//

	completedOutputPath = $state(initialSettings.completedOutputPath);
	recentRunLimit = $state(initialSettings.recentRunLimit);
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
		recentRunLimit: Math.min(20, Math.max(1, nonNegativeInt(this.recentRunLimit, 5))),
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
			monitorDesign: this.monitorDesign,
			showMonitorFps: this.showMonitorFps,
			showDeveloperSettings: this.showDeveloperSettings,
			showSourcePreviews: this.showSourcePreviews,
			lastUsedSourceName: this.lastUsedSourceName,
			welcomeModalShown: this.welcomeModalShown,
			completedOutputPath: this.completedOutputPath,
			recentRunLimit: Math.min(20, Math.max(1, nonNegativeInt(this.recentRunLimit, 5))),
			clipFilenameTemplate: this.clipFilenameTemplate,
			preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs, this.defaults.preRunPaddingSecs),
			postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, this.defaults.postRunPaddingSecs),
			discordNotificationsEnabled: this.discordNotificationsEnabled,
			discordWebhookUrl: this.discordWebhookUrl,
			streamingStartedMessageTemplate: this.streamingStartedMessageTemplate,
			streamingStoppedMessageTemplate: this.streamingStoppedMessageTemplate,
			updateCheckInterval: this.updateCheckInterval,
			lastUpdateCheckTime: this.lastUpdateCheckTime,
			autoUpdateEnabled: this.autoUpdateEnabled,
			youtubeVisibility: this.youtubeVisibility,
			youtubeTitleTemplate: this.youtubeTitleTemplate,
			youtubeDescriptionTemplate: this.youtubeDescriptionTemplate
		})
	);

	dirty = $derived(this.savedState !== this.lastSavedState);
	canEdit = $derived(this.loaded && this.fileError === null);

	saveImmediately(): void {
		void this.saveNow().catch((err) => {
			console.warn('Failed to save settings', err);
		});
	}

	async saveNow(): Promise<void> {
		if (!browser) return;
		if (!this.loaded) return;
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
				const saved = parseSettings(await backend.putSettings(snapshot), this.defaults);
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
		const reset = parseSettings(await backend.resetSettingsToDefaults(), this.defaults);
		this.apply(reset);
		this.lastSavedState = this.savedState;
		this.fileError = null;
		this.loaded = true;
		this.saveError = null;
	}

	async revealConfigFile(): Promise<void> {
		await backend.revealSettingsConfig();
		const status: SettingsStatus = await backend.getSettingsStatus();
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
		this.monitorDesign = next.monitorDesign;
		this.showMonitorFps = next.showMonitorFps;
		this.showDeveloperSettings = next.showDeveloperSettings;
		this.showSourcePreviews = next.showSourcePreviews;
		this.lastUsedSourceName = next.lastUsedSourceName;
		this.welcomeModalShown = next.welcomeModalShown;
		this.updateCheckInterval = next.updateCheckInterval;
		this.lastUpdateCheckTime = next.lastUpdateCheckTime;
		this.autoUpdateEnabled = next.autoUpdateEnabled;
		this.youtubeVisibility = next.youtubeVisibility;
		this.youtubeTitleTemplate = next.youtubeTitleTemplate;
		this.youtubeDescriptionTemplate = next.youtubeDescriptionTemplate;
		this.completedOutputPath = next.completedOutputPath;
		this.recentRunLimit = next.recentRunLimit;
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
