import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import RunCatalogSyncDialog from './RunCatalogSyncDialog.svelte';

describe('RunCatalogSyncDialog', () => {
	it('explains that an initial catalog build is normally a one-time task', () => {
		render(RunCatalogSyncDialog, { sync: 'initial' });

		expect(screen.getByRole('dialog', { name: 'Building your runs library' })).toBeInTheDocument();
		expect(screen.getByText(/normally only needed once/i)).toBeInTheDocument();
		expect(screen.getByRole('status')).toHaveTextContent(/reading clip details/i);
	});

	it('uses resync copy for a manual refresh', () => {
		render(RunCatalogSyncDialog, { sync: 'manual' });

		expect(screen.getByRole('dialog', { name: 'Resyncing your runs library' })).toBeInTheDocument();
		expect(screen.getByRole('status')).toHaveTextContent(/updating the run catalog/i);
		expect(screen.queryByText(/normally only needed once/i)).not.toBeInTheDocument();
	});
});
