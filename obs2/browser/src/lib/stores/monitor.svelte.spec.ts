import { beforeEach, describe, expect, it } from 'vitest';

import type { RecordingSaved } from '$lib/api';
import {
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
