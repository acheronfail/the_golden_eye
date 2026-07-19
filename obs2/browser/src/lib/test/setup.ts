import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach, vi } from 'vitest';

Element.prototype.scrollTo ??= () => {};

// Tests must mock backend API calls explicitly.
vi.stubGlobal(
	'fetch',
	vi.fn((input: RequestInfo | URL) => {
		const url = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url;
		throw new Error(`Unexpected fetch in frontend test: ${url}`);
	})
);

afterEach(() => {
	cleanup();
	vi.clearAllMocks();
});
