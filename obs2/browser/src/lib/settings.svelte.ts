import { browser } from '$app/environment';
import { z } from 'zod';

export const DEFAULT_CLIP_FILENAME_TEMPLATE = '{replay} - clip - {level}{time_suffix}{failed_suffix}';
export const DEFAULT_POST_RUN_PADDING_SECS = 5;

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

export const STORAGE_KEY = 'settings';
const storedSettings: Settings | null = (() => {
	if (!browser) return null;

	const stored = localStorage.getItem(STORAGE_KEY);
	if (!stored) return null;

	try {
		const parsed = JSON.parse(stored);
		return SettingsSchema.parse(parsed);
	} catch (e) {
		console.warn('Failed to parse stored settings, using defaults', e);
		return null;
	}
})();

const nonNegativeInt = (value: unknown): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : 0;
};

const nonNegativeNumber = (value: unknown, fallback = 0): number => {
	const n = Number(value);
	return Number.isFinite(n) ? Math.max(0, n) : fallback;
};

export const settings = new (class {
	//
	// Developer
	//

	developerLang = $state(storedSettings?.developerLang ?? 'en');

	//
	// Recording
	//

	completedOutputPath = $state(storedSettings?.completedOutputPath ?? '');
	saveFailedRuns = $state(storedSettings?.saveFailedRuns ?? true);
	failedOutputPath = $state(storedSettings?.failedOutputPath ?? '');
	failedRunLimit = $state(nonNegativeInt(storedSettings?.failedRunLimit ?? 0));
	clipFilenameTemplate = $state(storedSettings?.clipFilenameTemplate ?? DEFAULT_CLIP_FILENAME_TEMPLATE);
	preRunPaddingSecs = $state(nonNegativeNumber(storedSettings?.preRunPaddingSecs ?? 0));
	postRunPaddingSecs = $state(nonNegativeNumber(storedSettings?.postRunPaddingSecs ?? DEFAULT_POST_RUN_PADDING_SECS));

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
})();
