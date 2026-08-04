import { beforeEach, describe, expect, it } from 'vitest';

import type { AppSnapshot, RecordingSaved, RecordingStatus } from '$lib/api';
import {
	applyMonitorSnapshot,
	applyRecordingSaved,
	monitor,
	monitorPhaseStyleForPhase,
	monitorPresentationPhase
} from '$lib/stores/monitor.svelte';
import { notifications } from '$lib/stores/notifications.svelte';

const saved = (overrides: Partial<RecordingSaved> = {}): RecordingSaved => ({
	saveId: 1,
	path: '/clips/runway.mov',
	replayPath: '/clips/replay.mov',
	durationSecs: 12.3,
	failed: true,
	...overrides
});

const snapshot = (recordingState: RecordingStatus | null): AppSnapshot => ({
	monitor: {
		enabled: true,
		sourceName: 'N64 Capture',
		wallClocks: {
			sessionStartedAtUnixMs: null,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: null,
			levelElapsedMs: 0,
			levelRunning: false,
			levelPaused: false,
			levelStartReason: null,
			levelTimerPhase: 'idle',
			introSwirlDelayMs: null,
			fadeDetection: null
		}
	},
	match: null,
	runCatalogSync: null,
	recordingState,
	replaySaves: [],
	sources: [],
	replayBuffer: {} as AppSnapshot['replayBuffer'],
	settingsStatus: {} as AppSnapshot['settingsStatus'],
	update: {} as AppSnapshot['update']
});

describe('monitor presentation phases', () => {
	it('covers waiting and pre-monitor states separately', () => {
		expect(monitorPresentationPhase(null)).toBe('waiting');
		expect(monitorPresentationPhase(null, true)).toBe('neutral');
		expect(monitorPresentationPhase(null, false, false)).toBe('neutral');
		expect(monitorPhaseStyleForPhase('waiting').button).toBe('obs-phase-waiting-button');
		expect(monitorPhaseStyleForPhase('neutral').button).toBe('obs-phase-neutral-button');
	});

	it('maps every recording outcome to its chrome phase', () => {
		expect(monitorPresentationPhase('started')).toBe('recording');
		expect(monitorPresentationPhase('complete')).toBe('complete');
		expect(monitorPresentationPhase('cancelled')).toBe('neutral');
		for (const state of ['failed', 'aborted', 'kia', 'statsSkipped'] as const) {
			expect(monitorPresentationPhase(state)).toBe('danger');
		}
	});
});

describe('KIA overlay trigger', () => {
	beforeEach(() => {
		monitor.status = null;
		monitor.recordingState = null;
		monitor.kiaEffectId = 0;
	});

	it('triggers once when a snapshot enters KIA', () => {
		applyMonitorSnapshot(snapshot('started'));
		applyMonitorSnapshot(snapshot('kia'));

		expect(monitor.recordingState).toBe('kia');
		expect(monitor.kiaEffectId).toBe(1);
	});

	it('does not replay for repeated KIA snapshots', () => {
		applyMonitorSnapshot(snapshot('kia'));
		applyMonitorSnapshot(snapshot('kia'));

		expect(monitor.kiaEffectId).toBe(1);
	});

	it('triggers again when a later run enters KIA', () => {
		applyMonitorSnapshot(snapshot('kia'));
		applyMonitorSnapshot(snapshot('started'));
		applyMonitorSnapshot(snapshot('kia'));

		expect(monitor.kiaEffectId).toBe(2);
	});
});

describe('recording save events', () => {
	beforeEach(() => {
		notifications.flags = [];
	});

	it('clears the completed phase without adding a notification', () => {
		monitor.recordingState = 'kia';
		applyRecordingSaved(saved());
		expect(monitor.recordingState).toBeNull();
		expect(notifications.flags).toHaveLength(0);
	});
});
