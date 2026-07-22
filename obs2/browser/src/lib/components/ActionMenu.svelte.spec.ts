import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import ActionMenu from './ActionMenu.svelte';

afterEach(() => {
	vi.restoreAllMocks();
});

describe('ActionMenu', () => {
	it('opens above the trigger when the menu would leave the viewport', async () => {
		const user = userEvent.setup();
		vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
			top: 730,
			bottom: 770,
			left: 0,
			right: 40,
			width: 40,
			height: 40,
			x: 0,
			y: 730,
			toJSON: () => ({})
		});
		vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(160);
		render(ActionMenu, { items: [{ label: 'Open', action: () => {} }] });

		await user.click(screen.getByRole('button', { name: 'More actions' }));

		await waitFor(() => expect(screen.getByRole('menu')).toHaveClass('bottom-full'));
		expect(screen.getByRole('menu')).not.toHaveClass('top-full');
	});

	it('keeps the destructive action visually separate at the bottom', async () => {
		const user = userEvent.setup();
		render(ActionMenu, {
			items: [
				{ label: 'Open', action: () => {} },
				{ label: 'Delete', action: () => {}, tone: 'danger' }
			]
		});

		await user.click(screen.getByRole('button', { name: 'More actions' }));

		const actions = screen.getAllByRole('menuitem');
		expect(actions.at(-1)).toHaveTextContent('Delete');
		expect(actions.at(-1)).toHaveClass('obs-menu-link-danger');
	});

	it('keeps the trigger highlighted while its menu is open', async () => {
		const user = userEvent.setup();
		render(ActionMenu, { items: [{ label: 'Open', action: () => {} }] });
		const trigger = screen.getByRole('button', { name: 'More actions' });

		await user.click(trigger);
		expect(trigger).toHaveClass('obs-icon-button-open');

		await user.click(document.body);
		expect(trigger).not.toHaveClass('obs-icon-button-open');
	});
});
