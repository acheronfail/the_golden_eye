import { describe, expect, it } from 'vitest';
import type { LevelMatch } from '$lib/api';
import { monitorRunIdentityLabel, reconcileMonitorRunIdentity } from './monitorRunIdentity';

const match = (screen: string, mission = -1, part = -1, difficulty = -1): LevelMatch => ({
	screen,
	mission,
	part,
	difficulty,
	times: null,
	runtime_ms: 1
});

describe('monitor run identity', () => {
	it('starts empty and captures the detected start screen', () => {
		const empty = reconcileMonitorRunIdentity(null, match('unknown'));
		const facility = reconcileMonitorRunIdentity(empty, match('start', 1, 2, 2));

		expect(monitorRunIdentityLabel(empty)).toBe('- / -');
		expect(monitorRunIdentityLabel(facility)).toBe('Facility / 00 Agent');
	});

	it('captures identity from the 007 options launch screen and production casing', () => {
		const identity = reconcileMonitorRunIdentity(null, match('Opts007', 1, 1, 3));

		expect(monitorRunIdentityLabel(identity)).toBe('Dam / 007');
	});

	it('retains the identity during a run and replaces it on the next start', () => {
		const facility = reconcileMonitorRunIdentity(null, match('start', 1, 2, 2));
		const gameplay = reconcileMonitorRunIdentity(facility, match('unknown'));
		const dam = reconcileMonitorRunIdentity(gameplay, match('start', 1, 1, 0));

		expect(gameplay).toEqual(facility);
		expect(monitorRunIdentityLabel(dam)).toBe('Dam / Agent');
	});

	it.each(['levels', 'select'])('clears the identity on the %s screen', (screen) => {
		const facility = reconcileMonitorRunIdentity(null, match('start', 1, 2, 2));

		expect(reconcileMonitorRunIdentity(facility, match(screen))).toBeNull();
	});
});
