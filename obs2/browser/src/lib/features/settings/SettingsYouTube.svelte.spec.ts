import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { backend } from '$lib/api';
import { settings } from '$lib/stores/settings.svelte';
import { youtube } from '$lib/stores/youtube.svelte';
import { DEFAULT_SETTINGS } from '$lib/generated/settings';
import SettingsYouTube from './SettingsYouTube.svelte';

describe('automatic PB upload setting', () => {
	it('defaults to off and persists both enabling and disabling', async () => {
		settings.applyReloaded(DEFAULT_SETTINGS, '/tmp/settings.json', DEFAULT_SETTINGS);
		settings.loaded = true;
		youtube.connected = true;
		const save = vi.spyOn(backend, 'putSettings').mockImplementation(async (value) => value);
		const user = userEvent.setup();
		render(SettingsYouTube);
		const checkbox = screen.getByRole('checkbox', { name: 'Automatically upload personal bests (PBs)' });
		expect(checkbox).not.toBeChecked();
		await user.click(checkbox);
		await waitFor(() =>
			expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ youtubeAutoUploadPersonalBests: true }))
		);
		await user.click(checkbox);
		await waitFor(() =>
			expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ youtubeAutoUploadPersonalBests: false }))
		);
		save.mockRestore();
	});
});
