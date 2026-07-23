import { beforeEach, describe, expect, it, vi } from 'vitest';
import { notifications } from '$lib/stores/notifications.svelte';
import { settings } from '$lib/stores/settings.svelte';
import { updates } from '$lib/stores/updates.svelte';

const mocks = vi.hoisted(() => ({
	checkForUpdateNow: vi.fn(),
	downloadUpdateNow: vi.fn(),
	applyUpdateNow: vi.fn(),
	getUpdateStatus: vi.fn(),
	openUpdateRelease: vi.fn()
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			checkForUpdateNow: mocks.checkForUpdateNow,
			downloadUpdateNow: mocks.downloadUpdateNow,
			applyUpdateNow: mocks.applyUpdateNow,
			getUpdateStatus: mocks.getUpdateStatus,
			openUpdateRelease: mocks.openUpdateRelease
		}
	};
});

const available = {
	currentVersion: '0.5.0',
	latestVersion: '0.6.0-beta2',
	releaseUrl: 'https://example.com/0.6.0-beta2'
};

describe('update state', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		notifications.flags = [];
		settings.autoUpdateEnabled = false;
		updates.applyStatus({ phase: 'idle', available: null });
	});

	it('drives checking, download, and apply from authoritative backend status', async () => {
		mocks.checkForUpdateNow.mockResolvedValue({ update: available });
		mocks.downloadUpdateNow.mockResolvedValue(undefined);
		mocks.getUpdateStatus
			.mockResolvedValueOnce({ phase: 'available', available })
			.mockResolvedValueOnce({ phase: 'staged', available });

		await updates.check();
		expect(updates.buttonPhase).toBe('download');
		expect(notifications.flags).toEqual(
			expect.arrayContaining([expect.objectContaining({ title: 'Plugin update available' })])
		);

		await updates.download();
		expect(updates.buttonPhase).toBe('apply');
		expect(notifications.flags).toEqual(expect.arrayContaining([expect.objectContaining({ title: 'Update ready' })]));
	});

	it('projects background lifecycle transitions into the same button and notification state', () => {
		updates.applyStatus({ phase: 'downloading', available });
		expect(updates.buttonPhase).toBe('downloading');
		expect(notifications.flags).toEqual(
			expect.arrayContaining([expect.objectContaining({ title: 'Downloading update' })])
		);

		updates.applyStatus({ phase: 'applying', available });
		expect(updates.buttonPhase).toBe('applying');
		expect(notifications.flags).toEqual(
			expect.arrayContaining([expect.objectContaining({ title: 'Applying update' })])
		);
	});

	it('shows applying feedback immediately when apply is clicked', async () => {
		let finishApply!: () => void;
		mocks.applyUpdateNow.mockReturnValue(new Promise<void>((resolve) => (finishApply = resolve)));
		updates.applyStatus({ phase: 'staged', available });

		const applying = updates.apply();
		expect(updates.buttonPhase).toBe('applying');
		expect(notifications.flags).toEqual(
			expect.arrayContaining([expect.objectContaining({ title: 'Applying update' })])
		);

		finishApply();
		await expect(applying).resolves.toBe(true);
	});

	it('uses the applied event version rather than an earlier update', () => {
		updates.handleApplied('0.6.0-beta2');

		expect(notifications.flags).toEqual(
			expect.arrayContaining([expect.objectContaining({ title: 'Plugin updated', detail: 'Now running v0.6.0-beta2' })])
		);
	});

	it('shows the up-to-date result for 2.5 seconds', async () => {
		mocks.checkForUpdateNow.mockResolvedValue({ update: null });
		mocks.getUpdateStatus.mockResolvedValue({ phase: 'idle', available: null });

		await updates.check();

		expect(notifications.flags).toEqual([expect.objectContaining({ title: "You're up to date", timeoutMs: 2500 })]);
	});
});
