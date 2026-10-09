import { beforeEach, describe, expect, it } from 'vitest';

import type { AppSnapshot, LevelMatch, RecordingSaved, RecordingStatus } from '$lib/api';
import {
	applyMonitorSnapshot,
	applyMonitorStopped,
	monitorRunContext,
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

describe('monitor run context across navigation', () => {
	const start: LevelMatch = {
		screen: 'start',
		mission: 1,
		part: 2,
		difficulty: 2,
		detected_lang: 'en',
		times: null,
		runtime_ms: 1
	};
	const publish = (screen: LevelMatch['screen'], state: RecordingStatus | null = 'started') =>
		applyMonitorSnapshot({ ...snapshot(state), match: { ...start, screen } });

	beforeEach(() => {
		monitor.status = null;
		monitorRunContext.reset();
	});

	it('retains launch identity through gameplay snapshots while the view is absent', () => {
		publish('start');
		publish('unknown');
		publish('unknown');
		expect(monitorRunContext.identity).toEqual({ level: 'Facility', difficulty: '00 Agent' });
		expect(monitorRunContext.personalBestIdentity).toEqual(monitorRunContext.identity);
	});

	it('keeps PB hidden after stats while retaining the level until selection', () => {
		publish('start');
		publish('stats', 'complete');
		publish('unknown', 'complete');
		expect(monitorRunContext.identity?.level).toBe('Facility');
		expect(monitorRunContext.personalBestIdentity).toBeNull();
		publish('select', null);
		expect(monitorRunContext.identity).toBeNull();
	});

	it.each(['disabled', 'source changed', 'stopped'] as const)('clears identity when monitoring is %s', (reason) => {
		publish('start');
		if (reason === 'stopped') applyMonitorStopped('replayBufferStopped');
		else {
			const next = snapshot('started');
			next.match = { ...start, screen: 'unknown' };
			next.monitor.enabled = reason !== 'disabled';
			next.monitor.sourceName = reason === 'source changed' ? 'Other capture' : 'N64 Capture';
			applyMonitorSnapshot(next);
		}
		expect(monitorRunContext.identity).toBeNull();
		expect(monitorRunContext.personalBestIdentity).toBeNull();
	});
});
