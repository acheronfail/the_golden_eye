import { beforeEach, describe, expect, it } from 'vitest';

import type { RecordingSaved, RecordingSavePending } from '$lib/api';
import {
	applyFailedRunNotSaved,
	applyRecordingSaved,
	applyRecordingSaveDiscarded,
	applyRecordingSavePending,
	monitorPhaseStyleForPhase,
	monitorPresentationPhase
} from '$lib/stores/monitor.svelte';
import { notifications } from '$lib/stores/notifications.svelte';

const pending = (overrides: Partial<RecordingSavePending> = {}): RecordingSavePending => ({
	saveId: 1,
	saveInSecs: 5,
	estimatedDurationSecs: 20,
	failed: true,
	status: 'kia',
	level: 'Runway',
	difficulty: '00 Agent',
	timeSecs: 374,
	...overrides
});

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

describe('recording save notifications', () => {
	beforeEach(() => {
		notifications.flags = [];
	});

	it('replaces the pending notification in place when the run time is refined', () => {
		applyRecordingSavePending(pending({ timeSecs: 374 }));
		expect(notifications.flags).toHaveLength(1);
		const firstId = notifications.flags[0].id;
		expect(notifications.flags[0].detail).toContain('6:14');

		// Same saveId, corrected time: one toast, updated in place (no stale twin).
		applyRecordingSavePending(pending({ timeSecs: 14 }));
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].id).toBe(firstId);
		expect(notifications.flags[0].detail).toContain('0:14');
	});

	it('resolves the refined pending notification into the saved toast', () => {
		applyRecordingSavePending(pending({ timeSecs: 374 }));
		applyRecordingSavePending(pending({ timeSecs: 14 }));
		const pendingId = notifications.flags[0].id;

		applyRecordingSaved(saved());
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].id).toBe(pendingId);
		expect(notifications.flags[0].title).toBe('Clip saved');
	});

	it('dismisses the pending notification when the save is discarded', () => {
		applyRecordingSavePending(pending());
		expect(notifications.flags).toHaveLength(1);

		applyRecordingSaveDiscarded({ saveId: 1 });
		expect(notifications.flags).toHaveLength(0);
	});

	it('ignores a discard for a save with no pending notification', () => {
		applyRecordingSaveDiscarded({ saveId: 99 });
		expect(notifications.flags).toHaveLength(0);
	});

	it('surfaces a failed-run-not-saved outcome as a transient notification', () => {
		applyFailedRunNotSaved('tooShort');
		expect(notifications.flags).toHaveLength(1);
		expect(notifications.flags[0].title).toBe('Failed run not saved');
		expect(notifications.flags[0].detail).toContain('minimum');
		// Auto-dismissing, not a sticky phase indicator.
		expect(notifications.flags[0].timeoutMs).toBeGreaterThan(0);

		applyFailedRunNotSaved('savingDisabled');
		expect(notifications.flags).toHaveLength(2);
		expect(notifications.flags[1].detail).toContain('turned off');
	});
});
