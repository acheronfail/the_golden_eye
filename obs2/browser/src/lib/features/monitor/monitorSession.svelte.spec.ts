import { describe, expect, it, vi } from 'vitest';
import type { MonitorSessionActions, MonitorSessionSnapshot } from './monitorSession.svelte';
import { MonitorSessionController } from './monitorSession.svelte';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup(overrides: Partial<MonitorSessionActions> = {}) {
	const actions: MonitorSessionActions = {
		saveSettings: vi.fn().mockResolvedValue(undefined),
		refreshRecentRuns: vi.fn().mockResolvedValue(undefined),
		startMonitor: vi.fn().mockResolvedValue(undefined),
		stopMonitor: vi.fn().mockResolvedValue(undefined),
		refreshReplayBuffer: vi.fn(),
		navigate: vi.fn(),
		reportError: vi.fn(),
		stopPromptShown: () => true,
		saveStopPreference: vi.fn().mockResolvedValue(undefined),
		...overrides
	};
	return { controller: new MonitorSessionController(actions), actions };
}

function snapshot(overrides: Partial<MonitorSessionSnapshot> = {}): MonitorSessionSnapshot {
	return {
		sourceName: 'Capture',
		currentPath: '/sources/Capture',
		monitorLoaded: true,
		monitorStatus: { enabled: false, recordingState: null },
		sourcesLoaded: true,
		sourceExists: true,
		...overrides
	};
}

describe('MonitorSessionController', () => {
	it('starts a verified source and reaches the active phase', async () => {
		const { controller, actions } = setup();

		controller.reconcile(snapshot());
		expect(controller.phase.kind).toBe('starting');
		await flush();

		expect(actions.startMonitor).toHaveBeenCalledWith('Capture');
		expect(controller.phase).toEqual({ kind: 'active', confirmed: false });
		expect(controller.monitoring).toBe(true);
	});

	it('redirects to an already-active monitor without starting another', () => {
		const { controller, actions } = setup();

		controller.reconcile(
			snapshot({
				monitorStatus: { enabled: true, sourceName: 'Other Capture', recordingState: null }
			})
		);

		expect(controller.phase).toEqual({ kind: 'redirecting', href: '/sources/Other%20Capture' });
		expect(actions.navigate).toHaveBeenCalledWith('/sources/Other%20Capture', { replaceState: true });
		expect(actions.startMonitor).not.toHaveBeenCalled();
	});

	it('models stop preference errors without losing the active monitor', async () => {
		const { controller } = setup({
			stopPromptShown: () => false,
			saveStopPreference: vi.fn().mockRejectedValue(new Error('save failed'))
		});
		controller.phase = { kind: 'active', confirmed: true };

		controller.requestStop();
		expect(controller.stopPrompt.kind).toBe('open');
		await controller.chooseStopPreference(true);

		expect(controller.stopPrompt).toEqual({ kind: 'error', message: 'save failed' });
		expect(controller.phase.kind).toBe('active');
	});

	it('stops an active monitor and redirects home', async () => {
		const { controller, actions } = setup();
		controller.phase = { kind: 'active', confirmed: true };

		controller.requestStop();
		expect(controller.phase.kind).toBe('stopping');
		await flush();

		expect(actions.stopMonitor).toHaveBeenCalledOnce();
		expect(actions.navigate).toHaveBeenCalledWith('/', { replaceState: true });
	});
});
