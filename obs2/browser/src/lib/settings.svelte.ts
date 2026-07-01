import { browser } from '$app/environment';
import { z } from 'zod';
import { getSettings, putSettings } from './api';

export const DEFAULT_CLIP_FILENAME_TEMPLATE = '{level} - {time} - {difficulty} - {status}';
export const DEFAULT_POST_RUN_PADDING_SECS = 5;
const LEGACY_CLIP_FILENAME_TEMPLATE = '{replay} - clip - {level}{time_suffix}{failed_suffix}';

const SettingsSchema = z.object({
	developerLang: z.union([z.literal('en'), z.literal('jp')]).catch('en'),
	completedOutputPath: z.string().catch(''),
	saveFailedRuns: z.boolean().catch(true),
	failedOutputPath: z.string().catch(''),
	failedRunLimit: z.coerce.number().int().min(0).catch(0),
	clipFilenameTemplate: z.string().catch(DEFAULT_CLIP_FILENAME_TEMPLATE),
	preRunPaddingSecs: z.coerce.number().min(0).catch(0),
	postRunPaddingSecs: z.coerce.number().min(0).catch(DEFAULT_POST_RUN_PADDING_SECS)
});
export type Settings = z.infer<typeof SettingsSchema>;

export interface RecordingOptions {
	completedOutputPath: string;
	saveFailedRuns: boolean;
	failedOutputPath: string;
	failedRunLimit: number;
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

const parseSettings = (value: unknown): Settings => {
	const parsed = SettingsSchema.parse(value);
	return {
		...parsed,
		failedRunLimit: nonNegativeInt(parsed.failedRunLimit),
		clipFilenameTemplate: normalizeClipFilenameTemplate(parsed.clipFilenameTemplate),
		preRunPaddingSecs: nonNegativeNumber(parsed.preRunPaddingSecs),
		postRunPaddingSecs: nonNegativeNumber(parsed.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS)
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
	lastSavedState = $state(initialSavedState);

	private loadPromise: Promise<void> | null = null;
	private savePromise: Promise<void> | null = null;
	private saveQueued = false;

	//
	// Developer
	//

	developerLang = $state(initialSettings.developerLang);

	//
	// Recording
	//

	completedOutputPath = $state(initialSettings.completedOutputPath);
	saveFailedRuns = $state(initialSettings.saveFailedRuns);
	failedOutputPath = $state(initialSettings.failedOutputPath);
	failedRunLimit = $state(initialSettings.failedRunLimit);
	clipFilenameTemplate = $state(initialSettings.clipFilenameTemplate);
	preRunPaddingSecs = $state(initialSettings.preRunPaddingSecs);
	postRunPaddingSecs = $state(initialSettings.postRunPaddingSecs);

	recordingOptions: RecordingOptions = $derived({
		completedOutputPath: this.completedOutputPath.trim(),
		saveFailedRuns: this.saveFailedRuns,
		failedOutputPath: this.failedOutputPath.trim(),
		failedRunLimit: nonNegativeInt(this.failedRunLimit),
		clipFilenameTemplate: this.clipFilenameTemplate.trim() || DEFAULT_CLIP_FILENAME_TEMPLATE,
		preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs),
		postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS)
	});

	//
	// Stored
	//

	savedState = $derived(
		JSON.stringify({
			developerLang: this.developerLang,
			completedOutputPath: this.completedOutputPath,
			saveFailedRuns: this.saveFailedRuns,
			failedOutputPath: this.failedOutputPath,
			failedRunLimit: nonNegativeInt(this.failedRunLimit),
			clipFilenameTemplate: this.clipFilenameTemplate,
			preRunPaddingSecs: nonNegativeNumber(this.preRunPaddingSecs),
			postRunPaddingSecs: nonNegativeNumber(this.postRunPaddingSecs, DEFAULT_POST_RUN_PADDING_SECS)
		})
	);

	dirty = $derived(this.savedState !== this.lastSavedState);

	async load(): Promise<void> {
		if (!browser) return;
		if (this.loadPromise) return this.loadPromise;

		this.loading = true;
		this.saveError = null;
		this.loadPromise = (async () => {
			try {
				const remote = parseSettings(await getSettings());

				if (this.dirty) {
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

	private snapshot(): Settings {
		return parseSettings(JSON.parse(this.savedState));
	}

	private apply(next: Settings): void {
		this.developerLang = next.developerLang;
		this.completedOutputPath = next.completedOutputPath;
		this.saveFailedRuns = next.saveFailedRuns;
		this.failedOutputPath = next.failedOutputPath;
		this.failedRunLimit = next.failedRunLimit;
		this.clipFilenameTemplate = normalizeClipFilenameTemplate(next.clipFilenameTemplate);
		this.preRunPaddingSecs = next.preRunPaddingSecs;
		this.postRunPaddingSecs = next.postRunPaddingSecs;
	}
})();
