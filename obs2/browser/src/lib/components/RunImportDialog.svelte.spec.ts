import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import RunImportDialog from './RunImportDialog.svelte';

const props = () => ({
	open: true,
	onClose: vi.fn(),
	onManual: vi.fn(),
	onElite: vi.fn()
});

describe('RunImportDialog', () => {
	it('collects a complete manual history entry including an optional YouTube URL', async () => {
		const user = userEvent.setup();
		const callbacks = props();
		render(RunImportDialog, callbacks);

		await user.type(screen.getByLabelText('Time'), '1:23');
		await user.click(screen.getByRole('combobox', { name: 'Level' }));
		await user.click(screen.getByRole('option', { name: 'Facility' }));
		await user.click(screen.getByRole('combobox', { name: 'Difficulty' }));
		await user.click(screen.getByRole('option', { name: '00 agent' }));
		await user.click(screen.getByRole('combobox', { name: /ROM version/ }));
		await user.click(screen.getByRole('option', { name: 'PAL' }));
		await user.type(screen.getByLabelText(/YouTube link/), 'https://youtu.be/abc_123');
		await user.click(screen.getByRole('button', { name: 'add time' }));

		expect(callbacks.onManual).toHaveBeenCalledWith(
			expect.objectContaining({
				level: 'Facility',
				difficulty: '00 Agent',
				time: '1:23',
				gameLanguage: 'en',
				romVersion: 'pal',
				youtubeUrl: 'https://youtu.be/abc_123'
			})
		);
	});

	it('keeps a visible loading indicator throughout an Elite import', () => {
		render(RunImportDialog, { ...props(), initialMode: 'elite', busy: 'elite' });

		expect(screen.getByText('https://rankings.the-elite.net/~acheronfail/goldeneye/history')).toBeInTheDocument();
		const button = screen.getByRole('button', { name: /importing all times/i });
		expect(button).toBeDisabled();
		expect(button.querySelector('.animate-spin')).toBeInTheDocument();
	});

	it('reports imported, skipped, and linked-video counts', () => {
		render(RunImportDialog, {
			...props(),
			initialMode: 'elite',
			result: { imported: 84, alreadyImported: 3, videos: 73 }
		});

		expect(screen.getByRole('status')).toHaveTextContent(
			'Imported 84 times with 73 YouTube videos. 3 already existed.'
		);
	});
});
