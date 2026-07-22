import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { LevelMatch, RecordingStatus } from '$lib/api';
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
		if (design === 'mission-glass') {
			expect(view.container.querySelector('.glass-detail')).toHaveClass('detail-hidden');
		}
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
