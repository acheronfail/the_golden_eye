import { browser } from '$app/environment';
import { z } from 'zod';

const SettingsSchema = z.object({
	developerLang: z.union([z.literal('en'), z.literal('jp')])
});
export type Settings = z.infer<typeof SettingsSchema>;

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

export const settings = new (class {
	//
	// Monitor
	//

	developerLang = $state(storedSettings?.developerLang ?? 'en');

	//
	// Stored
	//

	savedState = $derived(
		JSON.stringify({
			developerLang: this.developerLang
		})
	);
})();
