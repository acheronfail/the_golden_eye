import type { RunCatalogSync } from '$lib/api';

export const runCatalog = $state<{ sync: RunCatalogSync | null }>({
	sync: null
});

export const setRunCatalogSync = (sync: RunCatalogSync | null): void => {
	runCatalog.sync = sync;
};
