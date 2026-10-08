import { describe, expect, it, vi } from 'vitest';
import type { RunClip } from '$lib/api';
import { BestTimesStore } from './bestTimes.svelte';

const mocks = vi.hoisted(() => ({ getBestTimes: vi.fn() }));
vi.mock('$lib/api', () => ({ backend: { getBestTimes: mocks.getBestTimes } }));

const best = { runId: 'catalog-pb' } as RunClip;

describe('best times store', () => {
	it('ignores an older response after a catalog refresh', async () => {
		let resolveOld!: (items: RunClip[]) => void;
		mocks.getBestTimes.mockReturnValueOnce(
			new Promise<RunClip[]>((resolve) => {
				resolveOld = resolve;
			})
		);
		const store = new BestTimesStore();
		const oldRefresh = store.refresh();
		mocks.getBestTimes.mockResolvedValueOnce([best]);
		await store.refresh();
		resolveOld([]);
		await oldRefresh;
		expect(store.items).toEqual([best]);
	});

	it('clears stale best times when the catalog cannot be read', async () => {
		const store = new BestTimesStore();
		store.items = [best];
		mocks.getBestTimes.mockRejectedValueOnce(new Error('unavailable'));
		await store.refresh();
		expect(store.items).toEqual([]);
	});
});
