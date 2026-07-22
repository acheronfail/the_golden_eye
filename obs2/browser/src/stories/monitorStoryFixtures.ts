import type { LevelMatch } from '$lib/api';

export const monitorMatch = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
	screen,
	mission: 2,
	part: 1,
	difficulty: 0,
	detected_lang: 'en',
	times,
	runtime_ms: 8.4
});
