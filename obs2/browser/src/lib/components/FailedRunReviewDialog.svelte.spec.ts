import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { RunClip } from '$lib/api';
import FailedRunReviewDialog from './FailedRunReviewDialog.svelte';

const clip: RunClip = {
	path: '/runs/failed/dam.mov',
	fileName: 'dam.mov',
	directory: '/runs/failed',
	sizeBytes: 100,
	durationSecs: 20,
	metadata: {
		timestamp: '2026-07-23T00:00:00Z',
		time: '00:12',
		level: 'Dam',
		difficulty: 'Agent',
		status: 'failed',
		romLanguage: 'en',
		sourceName: 'N64',
		comment: '',
		pluginVersion: 'test'
	}
};

describe('FailedRunReviewDialog', () => {
	it('requires selection and keeps without invoking discard', async () => {
		const user = userEvent.setup();
		const keep = vi.fn();
		const discard = vi.fn();
		render(FailedRunReviewDialog, { open: true, clips: [clip], close: vi.fn(), keep, discard });

		const keepButton = screen.getByRole('button', { name: /Keep selected/i });
		expect(keepButton).toBeDisabled();
		await user.click(screen.getByRole('checkbox', { name: /Select dam\.mov/i }));
		await user.click(keepButton);

		expect(keep).toHaveBeenCalledWith(['/runs/failed/dam.mov']);
		expect(discard).not.toHaveBeenCalled();
	});

	it('leaves every clip unresolved when review later is chosen', async () => {
		const user = userEvent.setup();
		const close = vi.fn();
		const keep = vi.fn();
		const discard = vi.fn();
		render(FailedRunReviewDialog, { open: true, clips: [clip], close, keep, discard });

		await user.click(screen.getByRole('button', { name: /Review later/i }));
		expect(close).toHaveBeenCalledOnce();
		expect(keep).not.toHaveBeenCalled();
		expect(discard).not.toHaveBeenCalled();
	});
});
