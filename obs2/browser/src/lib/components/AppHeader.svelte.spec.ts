import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import AppHeader, { type AppHeaderLink } from './AppHeader.svelte';

const links: AppHeaderLink[] = [
	{ href: '/', label: 'Monitor' },
	{ href: '/runs', label: 'Runs' },
	{ href: '/options', label: 'Options' }
];

describe('AppHeader', () => {
	it('only highlights the current page when monitoring is active', () => {
		render(AppHeader, {
			links,
			currentPath: '/options',
			pluginVersion: 'test',
			activeMonitorHref: '/sources/Nintendo%2064',
			menuOpen: true
		});

		const monitor = screen.getByRole('link', { name: 'Monitor' });
		const options = screen.getByRole('link', { name: 'Options' });

		expect(monitor).not.toHaveClass('obs-phase-waiting-button');
		expect(monitor).not.toHaveClass('obs-menu-link-active');
		expect(monitor).not.toHaveAttribute('aria-current');
		expect(options).toHaveClass('obs-menu-link-active');
		expect(options).toHaveAttribute('aria-current', 'page');
	});

	it('uses the normal active-menu style on the monitor page', () => {
		render(AppHeader, {
			links,
			currentPath: '/',
			pluginVersion: 'test',
			activeMonitorHref: '/sources/Nintendo%2064',
			menuOpen: true
		});

		const monitor = screen.getByRole('link', { name: 'Monitor' });
		expect(monitor).toHaveClass('obs-menu-link-active');
		expect(monitor).not.toHaveClass('obs-phase-waiting-button');
		expect(monitor).toHaveAttribute('aria-current', 'page');
	});
});
