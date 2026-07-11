import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	// NOTE: must match CMakeLists.txt
	server: { port: 5173, strictPort: true },
	resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
	build: {
		rolldownOptions: {
			// SvelteKit's inline bundle strategy can intentionally emit non-ESM chunks
			// where import.meta is unavailable.
			checks: {
				emptyImportMeta: false
			}
		}
	},
	test: {
		expect: { requireAssertions: true },
		projects: [
			{
				extends: './vite.config.ts',
				test: {
					name: 'server',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			},
			{
				extends: './vite.config.ts',
				test: {
					name: 'client',
					environment: 'jsdom',
					setupFiles: ['./src/lib/test/setup.ts'],
					include: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			}
		]
	}
});
