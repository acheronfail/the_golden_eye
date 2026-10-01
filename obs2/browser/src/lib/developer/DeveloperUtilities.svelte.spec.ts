import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import DeveloperUtilities from './DeveloperUtilities.svelte';

vi.mock('$lib/api', () => ({
	backend: {
		apiUrl: (path: string) => path,
		setMonitorMatcherAnnotations: vi.fn().mockResolvedValue({}),
		setMonitorFrameDump: vi.fn().mockResolvedValue({})
	}
}));
vi.mock('./kiaPreview', () => ({ triggerKiaDeathOverlay: vi.fn() }));

afterEach(() => vi.unstubAllGlobals());

it.each(['linux_capture_card', 'browser_source'])('allows inspecting the video source %s', async (id) => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ json: async () => [{ name: 'Video input', id }] }));
	const user = userEvent.setup();
	render(DeveloperUtilities);
	await user.click(screen.getByRole('button', { name: 'Load sources' }));
	await user.click(await screen.findByRole('button', { name: 'Choose source' }));
	expect(screen.getByRole('button', { name: 'Get screenshot' })).toBeEnabled();
	expect(screen.getByRole('button', { name: 'match screenshot' })).toBeEnabled();
	expect(screen.queryByText('(not a video source)')).not.toBeInTheDocument();
});
