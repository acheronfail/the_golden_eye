import type { MonitorWallClockState } from '$lib/api';
import { describe, expect, it } from 'vitest';
import { formatWallClockTime, MonitorWallClocks, type AnimationClock } from './monitorWallClocks.svelte';

class FakeAnimationClock implements AnimationClock {
	time = 0;
	wallTime = 1_800_000_000_000;
	private callback: FrameRequestCallback | null = null;

	now = (): number => this.time;
	wallNow = (): number => this.wallTime;
	requestFrame = (callback: FrameRequestCallback): number => {
		this.callback = callback;
		return 1;
	};
	cancelFrame = (): void => {
		this.callback = null;
	};

	advance(milliseconds: number): void {
		this.time += milliseconds;
		this.wallTime += milliseconds;
		const callback = this.callback;
		this.callback = null;
		callback?.(this.time);
	}
}

const idleState: MonitorWallClockState = {
	sessionStartedAtUnixMs: null,
	sessionElapsedMs: 0,
	sessionRunning: false,
	levelStartedAtUnixMs: null,
	levelElapsedMs: 0,
	levelRunning: false,
	levelPaused: false,
	levelStartReason: null,
	levelTimerPhase: 'idle',
	introSwirlDelayMs: null,
	fadeDetection: null
};

describe('MonitorWallClocks', () => {
	it('stays idle until a backend snapshot arrives', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);
		clock.advance(5_000);
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 0,
			sessionRunning: false,
			levelElapsedMs: 0,
			levelTimerPhase: 'idle'
		});
	});

	it('uses backend elapsed time when stopping and resetting a session', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);
		timers.sync({ ...idleState, sessionRunning: true, sessionElapsedMs: 1_000 });
		clock.advance(2_000);
		expect(timers.sessionElapsedMs).toBe(3_000);
		timers.sync({ ...idleState, sessionElapsedMs: 2_750 });
		clock.advance(2_000);
		expect(timers.sessionElapsedMs).toBe(2_750);
		expect(timers.sessionRunning).toBe(false);
		timers.sync({ ...idleState, sessionRunning: true });
		expect(timers.sessionElapsedMs).toBe(0);
	});

	it('does not advance the timer phase while waiting for backend fade detection', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);
		timers.sync({ ...idleState, sessionRunning: true, levelTimerPhase: 'awaitingInitialBlack' });
		clock.advance(10_000);
		expect(timers.snapshot()).toMatchObject({
			levelElapsedMs: 0,
			levelRunning: false,
			levelTimerPhase: 'awaitingInitialBlack'
		});
	});

	it('seeds running and stopped timers from backend timestamps after a reload', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.sync({
			sessionStartedAtUnixMs: clock.wallTime - 61_234,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: clock.wallTime - 2_345,
			levelElapsedMs: 0,
			levelRunning: true,
			levelPaused: false,
			levelStartReason: 'fade',
			levelTimerPhase: 'running',
			introSwirlDelayMs: 4_000,
			fadeDetection: {
				detected: false,
				meanLuma: 74,
				darkPixelPercent: 8,
				sampleCount: 576,
				sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
			}
		});
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 61_234,
			sessionRunning: true,
			levelElapsedMs: 2_345,
			levelRunning: true,
			levelTimerPhase: 'running',
			introSwirlDelayMs: 4_000
		});

		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 62_234,
			levelElapsedMs: 3_345
		});

		timers.sync({
			sessionStartedAtUnixMs: clock.wallTime - 62_234,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: null,
			levelElapsedMs: 3_345,
			levelRunning: false,
			levelPaused: false,
			levelStartReason: 'fade',
			levelTimerPhase: 'stopped',
			introSwirlDelayMs: 4_000,
			fadeDetection: {
				detected: false,
				meanLuma: 74,
				darkPixelPercent: 8,
				sampleCount: 576,
				sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
			}
		});
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({
			sessionElapsedMs: 63_234,
			sessionRunning: true,
			levelElapsedMs: 3_345,
			levelRunning: false
		});
	});

	it('keeps a watch-paused level frozen until the backend resumes it', () => {
		const clock = new FakeAnimationClock();
		const timers = new MonitorWallClocks(clock);

		timers.sync({
			sessionStartedAtUnixMs: clock.wallTime - 10_000,
			sessionElapsedMs: 0,
			sessionRunning: true,
			levelStartedAtUnixMs: null,
			levelElapsedMs: 2_500,
			levelRunning: false,
			levelPaused: true,
			levelStartReason: 'fade',
			levelTimerPhase: 'running',
			introSwirlDelayMs: 4_000,
			fadeDetection: null
		});
		clock.advance(1_000);

		expect(timers.snapshot()).toMatchObject({
			levelElapsedMs: 2_500,
			levelRunning: false,
			levelPaused: true,
			levelTimerPhase: 'running'
		});
		timers.sync({
			...idleState,
			sessionRunning: true,
			levelElapsedMs: 2_500,
			levelStartedAtUnixMs: clock.wallTime - 2_500,
			levelRunning: true,
			levelTimerPhase: 'running',
			levelStartReason: 'fade'
		});
		clock.advance(1_000);
		expect(timers.snapshot()).toMatchObject({ levelElapsedMs: 3_500, levelRunning: true, levelPaused: false });
	});
});

describe('formatWallClockTime', () => {
	it('formats minutes, seconds, and milliseconds with fixed-width sub-minute fields', () => {
		expect(formatWallClockTime(0)).toBe('00:00:000');
		expect(formatWallClockTime(61_234)).toBe('01:01:234');
		expect(formatWallClockTime(6_001_009)).toBe('100:01:009');
	});
});
