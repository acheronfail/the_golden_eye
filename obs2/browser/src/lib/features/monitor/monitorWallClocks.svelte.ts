import type { BlackFrameSignal, LevelTimerPhase, LevelTimerStartReason, MonitorWallClockState } from '$lib/api';

export interface AnimationClock {
	now: () => number;
	wallNow: () => number;
	requestFrame: (callback: FrameRequestCallback) => number;
	cancelFrame: (id: number) => void;
}

export interface MonitorWallClockSnapshot {
	sessionElapsedMs: number;
	sessionRunning: boolean;
	levelElapsedMs: number;
	levelRunning: boolean;
	levelStartReason: LevelTimerStartReason | null;
	levelTimerPhase: LevelTimerPhase;
	introSwirlDelayMs: number | null;
	fadeDetection: BlackFrameSignal | null;
}

const browserClock: AnimationClock = {
	now: () => performance.now(),
	wallNow: () => Date.now(),
	requestFrame: (callback) => requestAnimationFrame(callback),
	cancelFrame: (id) => cancelAnimationFrame(id)
};

export class MonitorWallClocks {
	sessionElapsedMs = $state(0);
	sessionRunning = $state(false);
	levelElapsedMs = $state(0);
	levelRunning = $state(false);
	levelStartReason = $state<LevelTimerStartReason | null>(null);
	levelTimerPhase = $state<LevelTimerPhase>('idle');
	introSwirlDelayMs = $state<number | null>(null);
	fadeDetection = $state<BlackFrameSignal | null>(null);

	private sessionStartedAt: number | null = null;
	private levelStartedAt: number | null = null;
	private previousMonitoring = false;
	private previousScreen: string | null = null;
	private animationFrame: number | null = null;

	constructor(private readonly clock: AnimationClock = browserClock) {}

	reconcile(monitoring: boolean, screen: string | null): void {
		const now = this.clock.now();
		const normalizedScreen = screen?.trim().toLowerCase() ?? null;

		if (monitoring && !this.previousMonitoring) {
			this.sessionElapsedMs = 0;
			this.sessionStartedAt = now;
			this.sessionRunning = true;
			this.levelElapsedMs = 0;
			this.levelStartedAt = null;
			this.levelRunning = false;
			this.levelStartReason = null;
			this.levelTimerPhase = 'idle';
			this.introSwirlDelayMs = null;
			this.fadeDetection = null;
			this.previousScreen = null;
		} else if (!monitoring && this.previousMonitoring) {
			this.tick(now);
			this.sessionRunning = false;
			this.sessionStartedAt = null;
			this.stopLevel(now);
		}

		if (monitoring && normalizedScreen !== this.previousScreen) {
			this.reconcileScreen(normalizedScreen, now);
			this.previousScreen = normalizedScreen;
		}
		this.previousMonitoring = monitoring;
		this.ensureAnimation();
	}

	sync(state: MonitorWallClockState): void {
		const now = this.clock.now();
		const wallNow = this.clock.wallNow();
		this.sessionElapsedMs = this.syncedElapsed(
			state.sessionElapsedMs,
			state.sessionRunning,
			state.sessionStartedAtUnixMs,
			wallNow
		);
		this.sessionRunning = state.sessionRunning;
		this.sessionStartedAt = state.sessionRunning ? now - this.sessionElapsedMs : null;
		this.levelElapsedMs = this.syncedElapsed(
			state.levelElapsedMs,
			state.levelRunning,
			state.levelStartedAtUnixMs,
			wallNow
		);
		this.levelRunning = state.levelRunning;
		this.levelStartReason = state.levelStartReason;
		this.levelTimerPhase = state.levelTimerPhase;
		this.introSwirlDelayMs = state.introSwirlDelayMs;
		this.fadeDetection = state.fadeDetection;
		this.levelStartedAt = state.levelRunning ? now - this.levelElapsedMs : null;
		this.previousMonitoring = state.sessionRunning;
		this.previousScreen = null;
		this.ensureAnimation();
	}

	snapshot(): MonitorWallClockSnapshot {
		return {
			sessionElapsedMs: this.sessionElapsedMs,
			sessionRunning: this.sessionRunning,
			levelElapsedMs: this.levelElapsedMs,
			levelRunning: this.levelRunning,
			levelStartReason: this.levelStartReason,
			levelTimerPhase: this.levelTimerPhase,
			introSwirlDelayMs: this.introSwirlDelayMs,
			fadeDetection: this.fadeDetection
		};
	}

	destroy(): void {
		if (this.animationFrame != null) this.clock.cancelFrame(this.animationFrame);
		this.animationFrame = null;
	}

	private reconcileScreen(screen: string | null, now: number): void {
		if (screen === 'start') {
			this.levelElapsedMs = 0;
			this.levelStartedAt = null;
			this.levelRunning = false;
			this.levelStartReason = null;
			this.levelTimerPhase = 'awaitingInitialBlack';
			this.introSwirlDelayMs = null;
			this.fadeDetection = null;
			return;
		}
		if (screen !== 'unknown') {
			this.stopLevel(now);
		}
	}

	private stopLevel(now: number): void {
		if (this.levelRunning) this.levelElapsedMs = this.elapsedSince(this.levelStartedAt, now);
		this.levelRunning = false;
		this.levelStartedAt = null;
		this.levelTimerPhase = 'stopped';
	}

	private tick(now: number): void {
		if (this.sessionRunning) this.sessionElapsedMs = this.elapsedSince(this.sessionStartedAt, now);
		if (this.levelRunning) this.levelElapsedMs = this.elapsedSince(this.levelStartedAt, now);
	}

	private elapsedSince(startedAt: number | null, now: number): number {
		return startedAt == null ? 0 : Math.max(0, Math.floor(now - startedAt));
	}

	private syncedElapsed(frozenMs: number, running: boolean, startedAtUnixMs: number | null, wallNow: number): number {
		if (!running || startedAtUnixMs == null) return Math.max(0, Math.floor(frozenMs));
		return Math.max(frozenMs, Math.floor(wallNow - startedAtUnixMs), 0);
	}

	private ensureAnimation(): void {
		if (this.animationFrame != null || (!this.sessionRunning && !this.levelRunning)) return;
		this.animationFrame = this.clock.requestFrame((now) => {
			this.animationFrame = null;
			this.tick(now);
			this.ensureAnimation();
		});
	}
}

export const formatWallClockTime = (milliseconds: number): string => {
	const wholeMilliseconds = Math.max(0, Math.floor(milliseconds));
	const minutes = Math.floor(wholeMilliseconds / 60_000);
	const seconds = Math.floor((wholeMilliseconds % 60_000) / 1_000);
	const millis = wholeMilliseconds % 1_000;
	return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}:${millis
		.toString()
		.padStart(3, '0')}`;
};
