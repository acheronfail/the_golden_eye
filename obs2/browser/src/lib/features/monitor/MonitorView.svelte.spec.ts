import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { LevelMatch, RecordingStatus, RunClip } from '$lib/api';
import MonitorView from './MonitorView.svelte';
import type { MonitorDesign } from './monitorView';

const match = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
	screen,
	mission: 2,
	part: 1,
	difficulty: 0,
	detected_lang: 'en',
	times,
	runtime_ms: 8.4
});

const props = (design: MonitorDesign, recordingState: RecordingStatus | null, levelMatch: LevelMatch) => ({
	design,
	verified: true,
	monitoring: true,
	recordingState,
	match: levelMatch,
	onStop: () => {}
});

const recentRun: RunClip = {
	runId: 'recent-run',
	path: '/runs/facility.mov',
	fileName: 'facility.mov',
	directory: '/runs',
	sizeBytes: 1024,
	metadata: {
		timestamp: '2026-07-23T10:00:00Z',
		time: '00:58',
		level: 'Facility',
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		gameLanguage: 'en',
		sourceName: 'N64 Capture',
		comment: '',
		pluginVersion: 'test'
	},
	retentionState: 'pending',
	retentionReason: null
};

describe.each<MonitorDesign>(['signal-band', 'mission-glass'])('%s monitor', (design) => {
	it('shows session and level wall clocks with an immediate styled tooltip', async () => {
		render(MonitorView, props(design, 'started', match('start')));

		const timers = screen.getByRole(design === 'signal-band' ? 'group' : 'region', {
			name: 'Wall-clock timers'
		});
		expect(timers).toHaveTextContent('Time in session');
		expect(timers).toHaveTextContent('Time in level');
		expect(timers).toHaveTextContent('00:00:000');
		const levelTimerLabel = screen.getByText('Time in level');
		const levelTimerTrigger = levelTimerLabel.parentElement?.parentElement;
		expect(levelTimerTrigger).toHaveClass('cursor-help');
		expect(levelTimerLabel).not.toHaveAttribute('title');
		expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();

		await fireEvent.pointerEnter(levelTimerTrigger!);
		const tooltip = screen.getByRole('tooltip');
		expect(tooltip).toHaveTextContent("GoldenEye's in-game timer can be inconsistent");
		expect(tooltip).toHaveTextContent('only an estimate');
		expect(tooltip).toHaveTextContent('waits through the opening cutscenes');
		expect(tooltip).toHaveTextContent('stops at the next fade to black');
		expect(levelTimerTrigger).toHaveAttribute('aria-describedby', tooltip.id);

		await fireEvent.pointerLeave(levelTimerTrigger!);
		expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
		expect(timers.querySelector('[data-running="true"]')).not.toBeNull();
		expect(timers.querySelector('[data-running="false"]')).not.toBeNull();
	});

	it('uses the neutral OBS-transition palette while verifying the source', () => {
		const view = render(MonitorView, {
			...props(design, null, match('unknown')),
			verified: false,
			monitoring: false
		});

		expect(view.container.querySelector('main')).toHaveAttribute('data-phase', 'neutral');
		expect(screen.getByRole('heading', { name: /^checking source$/i })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /stop monitoring/i })).toBeDisabled();
	});

	it('does not replay the transition when only the detected screen changes', async () => {
		const view = render(MonitorView, props(design, 'started', match('start')));
		const animatedSelector = design === 'signal-band' ? '.signal-content' : '.glass-panel';
		const animatedContent = view.container.querySelector(animatedSelector);

		await view.rerender(props(design, 'started', match('unknown')));

		expect(view.container.querySelector(animatedSelector)).toBe(animatedContent);
		expect(screen.getByRole('heading', { name: /^recording$/i })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /stop monitoring/i })).toBeEnabled();
		const detailSelector = design === 'signal-band' ? '.signal-detail' : '.glass-detail';
		expect(view.container.querySelector(detailSelector)).toHaveClass('invisible');
	});

	it('shows the start-screen level until level selection is shown', async () => {
		const start = { ...match('start'), mission: 1, part: 2, difficulty: 2 };
		const view = render(MonitorView, props(design, 'started', start));

		expect(screen.getByText('Facility / 00 Agent')).toHaveAttribute('data-available', 'true');

		await view.rerender(props(design, 'started', match('unknown')));
		expect(screen.getByText('Facility / 00 Agent')).toBeInTheDocument();

		await view.rerender(props(design, null, match('levels')));
		expect(screen.getByText('- / -')).toHaveAttribute('data-available', 'false');
		expect(screen.queryByText(/monitoring \/ active/i)).not.toBeInTheDocument();
	});

	it('keeps subtly styled stat placeholders mounted when run times appear', async () => {
		const view = render(MonitorView, props(design, 'started', match('unknown')));
		const selector = design === 'signal-band' ? '.signal-metrics' : '.glass-metrics';
		const metrics = view.container.querySelector(selector);

		expect(metrics).toHaveTextContent(/time\s+--:--\s+target\s+--:--\s+best\s+--:--/);
		expect(metrics?.querySelectorAll('[data-available="false"]')).toHaveLength(3);

		await view.rerender(props(design, 'complete', match('stats', { time: 58, target_time: 65, best_time: 61 })));

		const updatedMetrics = view.container.querySelector(selector);
		if (design === 'signal-band') expect(updatedMetrics).toBe(metrics);
		expect(updatedMetrics).toHaveTextContent(/time\s+0:58\s+target\s+1:05\s+best\s+1:01/);
		expect(updatedMetrics?.querySelectorAll('[data-available="true"]')).toHaveLength(3);
	});

	it('lands on the newest state when updates arrive faster than the animation', async () => {
		const view = render(MonitorView, props(design, null, match('unknown')));

		await view.rerender(props(design, 'started', match('start')));
		await view.rerender(props(design, 'failed', match('failed')));
		await view.rerender(props(design, 'complete', match('stats', { time: 58, target_time: 65, best_time: 61 })));

		expect(screen.getByRole('heading', { name: /^complete$/i })).toBeInTheDocument();
		expect(screen.queryByRole('heading', { name: /^recording$/i })).not.toBeInTheDocument();
		expect(screen.queryByRole('heading', { name: /^failed$/i })).not.toBeInTheDocument();
		expect(screen.getByText('0:58')).toBeInTheDocument();
	});

	it('uses backend throughput health for FPS warning colours', async () => {
		const view = render(MonitorView, {
			...props(design, 'started', match('start')),
			showMonitorFps: true,
			fps: { processedFps: 29, capturedFps: 30, sourceFps: 30, droppedFrames: 1, health: 'warning' }
		});
		const meter = screen.getByText('29.0 / 30.0 FPS');
		expect(meter).toHaveClass('text-amber-400');
		expect(meter).not.toHaveClass('text-(--obs-danger)');

		await view.rerender({
			...props(design, 'started', match('start')),
			showMonitorFps: true,
			fps: { processedFps: 25, capturedFps: 30, sourceFps: 30, droppedFrames: 3, health: 'lagging' }
		});
		expect(screen.getByText('25.0 / 30.0 FPS')).toHaveClass('text-(--obs-danger)');
	});
});

describe('debug monitor', () => {
	it('shows all available recording diagnostics without animation wrappers', () => {
		const levelMatch: LevelMatch = {
			...match('stats', { time: 58, target_time: 65, best_time: 61 }),
			raw_times: [58, 65, 61],
			match_regions: [{ label: 'time', x: 10, y: 20, w: 30, h: 40, score: 0.98 }]
		};
		const view = render(MonitorView, {
			...props('debug', 'complete', levelMatch),
			sourceName: 'N64 Capture',
			cvLanguage: 'jp',
			replaySaves: [
				{
					trackingId: 42,
					saveId: 8,
					stage: 'savingReplay',
					level: 'Facility',
					difficulty: '00 Agent',
					runStatus: 'complete',
					estimatedDurationSecs: 68
				},
				{
					trackingId: 41,
					saveId: 7,
					stage: 'trimming',
					level: 'Dam',
					difficulty: 'Agent',
					runStatus: 'failed',
					estimatedDurationSecs: 82
				}
			],
			showMonitorFps: true,
			fps: { processedFps: 59, capturedFps: 60, sourceFps: 30, droppedFrames: 1, health: 'warning' }
		});

		expect(screen.getByRole('heading', { name: /^complete$/i })).toBeInTheDocument();
		expect(screen.getByText('N64 Capture')).toBeInTheDocument();
		expect(screen.getByText('CV language')).toBeInTheDocument();
		expect(screen.getByText('jp')).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Replay buffer handling' })).toBeInTheDocument();
		expect(screen.getByText('saving replay')).toBeInTheDocument();
		expect(screen.getByText('trimming')).toBeInTheDocument();
		expect(screen.getByText('#8')).toBeInTheDocument();
		expect(screen.getByText('#7')).toBeInTheDocument();
		const fpsMonitor = screen.getByText('59.0 / 60.0 FPS');
		expect(fpsMonitor.closest('[data-fps-health]')).toHaveAttribute('data-fps-health', 'warning');
		expect(fpsMonitor.closest('dd')).toHaveClass('text-amber-400');
		expect(screen.getByText('processed FPS').nextElementSibling).toHaveTextContent('59');
		expect(screen.getByText('captured FPS').nextElementSibling).toHaveTextContent('60');
		expect(screen.getByText('configured FPS').nextElementSibling).toHaveTextContent('30');
		expect(screen.getByText('dropped frames').nextElementSibling).toHaveTextContent('1');
		expect(screen.getByText('health').nextElementSibling).toHaveTextContent('warning');
		expect(screen.getByText('[58,65,61]')).toBeInTheDocument();
		expect(screen.getByText(/"score": 0.98/)).toBeInTheDocument();
		expect(screen.queryByText(/show FPS setting/i)).not.toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'Wall-clock timers' })).toBeInTheDocument();
		expect(screen.getByText('time in session')).toBeInTheDocument();
		expect(screen.getByText('time in level')).toBeInTheDocument();
		expect(screen.getByText('level timer origin')).toBeInTheDocument();
		expect(screen.getByText('level timer phase')).toBeInTheDocument();
		expect(screen.getByText('intro swirl delay')).toBeInTheDocument();
		expect(screen.getByText('black frame')).toBeInTheDocument();
		expect(screen.getByText('sampled mean luma')).toBeInTheDocument();
		expect(screen.getByText('dark sample coverage')).toBeInTheDocument();
		expect(screen.getByText('active picture sample')).toBeInTheDocument();
		expect(view.container.querySelectorAll('.state-cell')).toHaveLength(11);
		expect(view.container.querySelectorAll('[data-value-kind="true"]')).not.toHaveLength(0);
		expect(view.container.querySelectorAll('[data-value-kind="null"]')).not.toHaveLength(0);
		expect(view.container.querySelector('[class*="motion"], [class*="sweep"]')).not.toBeInTheDocument();
	});
});

describe.each<MonitorDesign>(['mission-glass', 'signal-band', 'debug'])('%s recent runs', (design) => {
	it('places run history inside the design layout', () => {
		const view = render(MonitorView, {
			...props(design, 'complete', match('stats')),
			recentRuns: [recentRun]
		});
		const history = screen.getByRole('region', { name: 'Recent runs' });

		expect(history).toHaveClass(`recent-runs--${design}`);
		expect(history.querySelector('.run-scroll')).not.toBeNull();
		if (design === 'mission-glass') {
			expect(history.closest('.glass-layout')).toHaveClass(
				'h-[calc(100cqh-9rem)]',
				'grid-rows-[auto_auto_minmax(0,1fr)]'
			);
			expect(history).toHaveClass('h-full', 'max-h-none');
		}
		if (design === 'signal-band') {
			expect(history.closest('.signal-layout')).not.toBeNull();
			expect(history).toHaveClass('@max-[760px]:h-full', '@max-[760px]:max-h-none');
		}
		if (design === 'debug') {
			const lifecycle = view.container.querySelector('[aria-labelledby="lifecycle-heading"]');
			expect(history.compareDocumentPosition(lifecycle!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
		}
	});
});
