import { browser } from '$app/environment';
import { z } from 'zod';

const SettingsSchema = z.object({
	obsUrl: z.string(),
	obsPassword: z.string()
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
	// OBS
	//

	obsUrl = $state(storedSettings?.obsUrl ?? 'ws://localhost:4455');
	obsPassword = $state(storedSettings?.obsPassword ?? '');

	//
	// Stored
	//

	savedState = $derived(
		JSON.stringify({
			obsUrl: this.obsUrl,
			obsPassword: this.obsPassword
		})
	);
})();
