import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import AppHeader, { type AppHeaderLink } from './AppHeader.svelte';

const links: AppHeaderLink[] = [
	{ href: '/', label: 'Monitor' },
	{ href: '/runs', label: 'Runs' },
	{ href: '/options', label: 'Options' }
];

describe('AppHeader', () => {
	it('highlights the active monitor session while preserving the current page', () => {
		render(AppHeader, {
			links,
			currentPath: '/options',
			pluginVersion: 'test',
			activeMonitorHref: '/sources/Nintendo%2064',
			menuOpen: true
		});

		const monitor = screen.getByRole('link', { name: 'Monitor' });
		const options = screen.getByRole('link', { name: 'Options' });

		expect(monitor).toHaveClass('obs-phase-waiting-button');
		expect(monitor).not.toHaveAttribute('aria-current');
		expect(options).toHaveClass('obs-menu-link-active');
		expect(options).toHaveAttribute('aria-current', 'page');
	});
});
