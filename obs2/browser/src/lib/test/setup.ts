import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

Element.prototype.scrollTo ??= () => {};

afterEach(() => {
	cleanup();
});
