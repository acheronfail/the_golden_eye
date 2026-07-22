import type { ObsSource } from '$lib/api';

export const obsSources = $state<{
	items: ObsSource[] | null;
	loaded: boolean;
	version: number;
}>({
	items: null,
	loaded: false,
	version: 0
});

export const setObsSources = (sources: ObsSource[]): void => {
	obsSources.items = sources;
	obsSources.loaded = true;
	obsSources.version += 1;
};
