import { describe, expect, it, vi } from 'vitest';
import type { RunClip } from '$lib/api';
import { RunsPageController, type RunsPageNavigation } from './runsPageController.svelte';

const clip = {
	runId: 'run-1',
	path: '/runs/old.mp4',
	fileName: 'old.mp4',
	directory: '/runs',
	retentionState: 'pending',
	metadata: {
		gameLanguage: 'en',
		romVersion: 'ntsc-u',
		status: 'complete',
		difficulty: 'Agent',
		time: '01:05',
		level: 'Dam',
		timestamp: '2026-01-01T00:00:00Z'
	}
} as RunClip;

function setup() {
	const api = {
		getRuns: vi.fn().mockResolvedValue({ directories: [], clips: [clip] }),
		updateRunMetadata: vi.fn(),
		renameRun: vi.fn().mockResolvedValue({ ...clip, path: '/runs/new.mp4', fileName: 'new.mp4' }),
		revealRun: vi.fn().mockResolvedValue(undefined),
		revealRunFolder: vi.fn().mockResolvedValue(undefined),
		deleteCatalogRun: vi.fn(),
		keepRun: vi.fn().mockResolvedValue({ ...clip, retentionState: 'kept' }),
		createManualRun: vi.fn(),
		importTheElite: vi.fn()
	};
	const navigation: RunsPageNavigation = {
		currentUrl: () => new URL('http://localhost/runs'),
		goto: vi.fn(),
		promptForFilename: vi.fn().mockReturnValue('new.mp4')
	};
	const controller = new RunsPageController(api, navigation);
	controller.runs = { directories: [], clips: [clip] };
	return { controller, api, navigation };
}

describe('RunsPageController', () => {
	it('updates the catalog and selected run through one rename path', async () => {
		const { controller } = setup();
		controller.select(clip);
		controller.metadataDraft!.time = '01:06';

		await controller.detailView.actions.rename();

		expect(controller.clips[0].fileName).toBe('new.mp4');
		expect(controller.selected?.fileName).toBe('new.mp4');
		expect(controller.metadataDraft?.time).toBe('01:06');
	});

	it('uses run ids consistently for list action state', async () => {
		const { controller } = setup();

		await controller.keepFromList(clip);

		expect(controller.clips[0].retentionState).toBe('kept');
		expect(controller.listActionBusyId).toBeNull();
	});

	it('removes a deleted run from both the catalog and selection', async () => {
		const { controller, api } = setup();
		api.deleteCatalogRun.mockResolvedValue(null);
		controller.select(clip);
		controller.requestDelete(clip);

		await controller.confirmDelete(false);

		expect(controller.clips).toEqual([]);
		expect(controller.selected).toBeNull();
		expect(controller.metadataDraft).toBeNull();
	});
});
