import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach, vi } from 'vitest';

Element.prototype.scrollTo ??= () => {};
window.matchMedia ??= (query: string) =>
	({
		matches: false,
		media: query,
		onchange: null,
		addEventListener: vi.fn(),
		removeEventListener: vi.fn(),
		addListener: vi.fn(),
		removeListener: vi.fn(),
		dispatchEvent: vi.fn(() => false)
	}) as MediaQueryList;

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
