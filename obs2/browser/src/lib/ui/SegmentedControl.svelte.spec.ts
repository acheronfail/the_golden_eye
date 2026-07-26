import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import SegmentedControl from './SegmentedControl.svelte';

const options = [
	{ value: 'share', label: 'Share' },
	{ value: 'count', label: 'Count' },
	{ value: 'rate', label: 'Rate' }
];

describe('SegmentedControl', () => {
	it('selects an option and reports the change', async () => {
		const user = userEvent.setup();
		const onChange = vi.fn();
		render(SegmentedControl, {
			value: 'share',
			options,
			ariaLabel: 'Measure',
			onChange
		});

		await user.click(screen.getByRole('radio', { name: 'Count' }));

		expect(screen.getByRole('radio', { name: 'Count' })).toHaveAttribute('aria-checked', 'true');
		expect(onChange).toHaveBeenCalledWith('count');
	});

	it('supports radiogroup arrow, Home, and End navigation', async () => {
		const user = userEvent.setup();
		render(SegmentedControl, {
			value: 'share',
			options,
			ariaLabel: 'Measure'
		});
		const share = screen.getByRole('radio', { name: 'Share' });
		share.focus();

		await user.keyboard('{ArrowLeft}');
		expect(screen.getByRole('radio', { name: 'Rate' })).toHaveFocus();
		expect(screen.getByRole('radio', { name: 'Rate' })).toHaveAttribute('aria-checked', 'true');

		await user.keyboard('{Home}');
		expect(share).toHaveFocus();

		await user.keyboard('{End}');
		expect(screen.getByRole('radio', { name: 'Rate' })).toHaveFocus();
	});
});
