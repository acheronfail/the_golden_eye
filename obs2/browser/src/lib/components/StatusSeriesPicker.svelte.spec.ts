import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import StatusSeriesPicker from './StatusSeriesPicker.svelte';

describe('StatusSeriesPicker', () => {
	it('allows statuses to be hidden while keeping at least one visible', async () => {
		const user = userEvent.setup();
		render(StatusSeriesPicker, { value: ['complete', 'failed'] });
		const complete = screen.getByRole('checkbox', { name: 'Show Complete runs' });
		const failed = screen.getByRole('checkbox', { name: 'Show Failed runs' });

		await user.click(failed);
		expect(failed).not.toBeChecked();
		expect(complete).toBeChecked();
		expect(complete).toBeDisabled();
	});
});
