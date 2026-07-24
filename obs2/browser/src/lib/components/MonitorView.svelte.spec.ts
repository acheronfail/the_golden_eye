import { render, screen } from '@testing-library/svelte';
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
		romLanguage: 'en',
		sourceName: 'N64 Capture',
		comment: '',
		pluginVersion: 'test'
	},
	retentionState: 'pending',
	retentionReason: null
};

describe.each<MonitorDesign>(['signal-band', 'mission-glass'])('%s monitor', (design) => {
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
			fps: { processedFps: 60, sourceFps: 60 }
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
		expect(screen.getAllByText('60')).toHaveLength(2);
		expect(screen.getByText('[58,65,61]')).toBeInTheDocument();
		expect(screen.getByText(/"score": 0.98/)).toBeInTheDocument();
		expect(screen.queryByText(/show FPS setting/i)).not.toBeInTheDocument();
		expect(view.container.querySelectorAll('.state-cell')).toHaveLength(7);
		expect(view.container.querySelectorAll('[data-value-kind="true"]')).not.toHaveLength(0);
		expect(view.container.querySelectorAll('[data-value-kind="false"]')).not.toHaveLength(0);
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
		if (design === 'mission-glass') expect(history.closest('.glass-layout')).not.toBeNull();
		if (design === 'signal-band') expect(history.closest('.signal-layout')).not.toBeNull();
		if (design === 'debug') {
			const lifecycle = view.container.querySelector('[aria-labelledby="lifecycle-heading"]');
			expect(history.compareDocumentPosition(lifecycle!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
		}
	});
});
