import { chromium } from 'playwright';

try {
	const browser = await chromium.launch({ headless: true });
	await browser.close();
} catch (error) {
	console.error('Chromium is unavailable. Run "just setup-browser" from the repository root.');
	console.error(error.message);
	process.exitCode = 1;
}
