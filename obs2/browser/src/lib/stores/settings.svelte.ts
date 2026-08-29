import { browser } from '$app/environment';
import { backend, type SettingsStatus } from '$lib/api';
import {
	DEFAULT_SETTINGS,
	MAX_RECENT_RUN_LIMIT,
	type AppSettings,
	type MonitorDesign,
	type UpdateCheckInterval,
	type YoutubeVisibility
} from '$lib/generated/settings';

export {
	MAX_RECENT_RUN_LIMIT,
	type AppSettings as Settings,
	type MonitorDesign,
	type UpdateCheckInterval,
	type YoutubeVisibility
};

export interface RecordingOptions {
	completedOutputPath: string;
	recentRunLimit: number;
	clipFilenameTemplate: string;
	preRunPaddingSecs: number;
	postRunPaddingSecs: number;
}

const settingKeys = Object.keys(DEFAULT_SETTINGS) as (keyof AppSettings)[];

const copySettings = (value: AppSettings): AppSettings =>
	Object.fromEntries(settingKeys.map((key) => [key, structuredClone(value[key])])) as unknown as AppSettings;

const nonNegativeInt = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : fallback;
};

const nonNegativeNumber = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, n) : fallback;
};

const serializeSettings = (value: AppSettings): string => JSON.stringify(copySettings(value));
const initialSettings = copySettings(DEFAULT_SETTINGS);
const initialSavedState = serializeSettings(initialSettings);
const errorMessage = (err: unknown): string => (err instanceof Error ? err.message : String(err));

export const settings = new (class {
	loaded = $state(!browser);
	saving = $state(false);
	saveError = $state<string | null>(null);
	configPath = $state('');
	fileError = $state<string | null>(null);
	pluginVersion = $state('0.0.0-unknown');
	lastSavedState = $state(initialSavedState);
	defaults = $state<AppSettings>(copySettings(initialSettings));
	values = $state<AppSettings>(copySettings(initialSettings));

	private savePromise: Promise<void> | null = null;
	private saveQueued = false;

	recordingOptions: RecordingOptions = $derived({
		completedOutputPath: this.values.completedOutputPath.trim(),
		recentRunLimit: Math.min(
			MAX_RECENT_RUN_LIMIT,
			Math.max(1, nonNegativeInt(this.values.recentRunLimit, this.defaults.recentRunLimit))
		),
		clipFilenameTemplate: this.values.clipFilenameTemplate.trim() || this.defaults.clipFilenameTemplate,
		preRunPaddingSecs: nonNegativeNumber(this.values.preRunPaddingSecs, this.defaults.preRunPaddingSecs),
		postRunPaddingSecs: nonNegativeNumber(this.values.postRunPaddingSecs, this.defaults.postRunPaddingSecs)
	});

	savedState = $derived(serializeSettings(this.values));
	dirty = $derived(this.savedState !== this.lastSavedState);
	canEdit = $derived(this.loaded && this.fileError === null);

	saveImmediately(): void {
		void this.saveNow().catch((err) => {
			console.warn('Failed to save settings', err);
		});
	}

	async saveNow(): Promise<void> {
		if (!browser || !this.loaded || this.fileError !== null || !this.dirty) return;

		if (this.savePromise) {
			this.saveQueued = true;
			await this.savePromise;
			if (this.dirty) await this.saveNow();
			return;
		}

		const snapshot = copySettings(this.values);
		const snapshotState = JSON.stringify(snapshot);
		if (snapshotState === this.lastSavedState) return;

		this.saving = true;
		this.saveError = null;
		this.savePromise = (async () => {
			try {
				const saved = copySettings(await backend.putSettings(snapshot));
				const savedState = JSON.stringify(saved);
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
		this.defaults = copySettings(status.defaults);
		this.apply(status.settings);
		this.lastSavedState = this.savedState;
		this.loaded = true;
		this.saveError = null;
	}

	applyReloaded(next: AppSettings, configPath: string, defaults = this.defaults): void {
		this.configPath = configPath;
		this.fileError = null;
		this.defaults = copySettings(defaults);
		this.apply(next);
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
		this.apply(await backend.resetSettingsToDefaults());
		this.lastSavedState = this.savedState;
		this.fileError = null;
		this.loaded = true;
		this.saveError = null;
	}

	async revealConfigFile(): Promise<void> {
		await backend.revealSettingsConfig();
		const status = await backend.getSettingsStatus();
		this.defaults = copySettings(status.defaults);
		this.configPath = status.configPath;
		this.fileError = status.fileError ?? null;
		this.pluginVersion = status.pluginVersion;
	}

	private apply(next: AppSettings): void {
		this.values = copySettings(next);
	}
})();
