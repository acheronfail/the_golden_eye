import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import ReadClipsDialog from './ReadClipsDialog.svelte';

describe('ReadClipsDialog', () => {
	it('explains the rescan before confirming it', async () => {
		const user = userEvent.setup();
		const read = vi.fn();
		render(ReadClipsDialog, { cancel: vi.fn(), read });

		expect(screen.getByRole('dialog', { name: 'Read clips?' })).toHaveTextContent(/new tagged clips are added/i);
		expect(screen.getByRole('dialog', { name: 'Read clips?' })).toHaveTextContent(/will not delete any video files/i);
		expect(read).not.toHaveBeenCalled();

		await user.click(screen.getByRole('button', { name: 'Read clips' }));
		expect(read).toHaveBeenCalledOnce();
	});
});
