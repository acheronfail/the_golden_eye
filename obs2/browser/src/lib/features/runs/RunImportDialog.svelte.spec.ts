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
		const language = screen.getByRole('combobox', { name: 'Game language' });
		await user.click(screen.getByRole('combobox', { name: /ROM version/ }));
		await user.click(screen.getByRole('option', { name: 'NTSC-J' }));
		expect(language).toBeDisabled();
		expect(language).toHaveTextContent('jp');
		await user.type(screen.getByLabelText(/YouTube link/), 'https://youtu.be/abc_123');
		await user.click(screen.getByRole('button', { name: 'Add time' }));

		expect(callbacks.onManual).toHaveBeenCalledWith(
			expect.objectContaining({
				level: 'Facility',
				difficulty: '00 Agent',
				time: '1:23',
				gameLanguage: 'jp',
				romVersion: 'ntsc-j',
				youtubeUrl: 'https://youtu.be/abc_123'
			})
		);
	});

	it('re-enables game language when the ROM version is cleared', async () => {
		const user = userEvent.setup();
		render(RunImportDialog, props());

		const language = screen.getByRole('combobox', { name: 'Game language' });
		const romVersion = screen.getByRole('combobox', { name: /ROM version/ });
		await user.click(romVersion);
		await user.click(screen.getByRole('option', { name: 'PAL' }));
		expect(language).toBeDisabled();
		expect(language).toHaveTextContent('en');

		await user.click(romVersion);
		await user.click(screen.getByRole('option', { name: 'not set' }));
		expect(language).toBeEnabled();
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
