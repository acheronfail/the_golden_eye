import { basename, dirname, relative } from 'node:path';
import adapter from '@sveltejs/adapter-static';

// Output location is the single source of truth defined by CMake via
// $BROWSER_BUNDLE. Required with no fallback so the path is never guessed.
const bundle = process.env.BROWSER_BUNDLE;
if (!bundle) {
	console.warn('BROWSER_BUNDLE is not set; build via CMake (or set the env var)');
}

const outDir = bundle && relative(import.meta.dirname, dirname(bundle));
const fallback = bundle && basename(bundle);

/** @type {import('@sveltejs/kit').Config} */
const config = {
	compilerOptions: {
		// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
		runes: ({ filename }) => (filename.split(/[/\\]/).includes('node_modules') ? undefined : true)
	},
	kit: {
		// Static build: emit a self-contained SPA into the CMake-defined output dir.
		adapter: adapter({
			pages: outDir,
			assets: outDir,
			fallback
		}),
		// Inline all JS/CSS into the HTML so the app ships as a single file,
		// making it easy to embed in a binary later.
		output: { bundleStrategy: 'inline' }
	}
};

export default config;
