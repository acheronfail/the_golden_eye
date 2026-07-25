import type { Preview } from '@storybook/sveltekit';
import { MINIMAL_VIEWPORTS } from 'storybook/viewport';
import '../src/routes/layout.css';
import './preview.css';

const preview: Preview = {
	parameters: {
		a11y: { test: 'todo' },
		backgrounds: { default: 'OBS' },
		// Monitor states are one file per state now; pin the lifecycle order so the
		// sidebar doesn't fall back to alphabetical. Other sections keep their order.
		options: {
			storySort: {
				order: [
					'*',
					'Monitor',
					[
						'Monitor states',
						[
							'Verifying source',
							'Starting monitor',
							'Waiting',
							'Recording',
							'Cancelled',
							'Failed',
							'Aborted',
							'Killed in action',
							'Complete',
							'Skipped stats',
							'Saving clip',
							'Overlapping replay handling',
							'Recent run history',
							'Stats with long recent history',
							'Stopping monitor',
							'Healthy monitor FPS',
							'Warning monitor FPS',
							'Lagging monitor FPS'
						],
						'*'
					]
				]
			}
		},
		viewport: {
			options: {
				obsDock: {
					name: 'OBS side dock',
					styles: { width: '420px', height: '900px' },
					type: 'desktop'
				},
				obsDockHalfHeight: {
					name: 'OBS side dock (half height)',
					styles: { width: '420px', height: '450px' },
					type: 'desktop'
				},
				...MINIMAL_VIEWPORTS
			}
		},
		controls: {
			matchers: {
				color: /(background|color)$/i,
				date: /Date$/i
			}
		}
	},
	initialGlobals: {
		viewport: { value: 'obsDock', isRotated: false }
	},
	globalTypes: {
		backgrounds: {
			defaultValue: 'OBS',
			items: [{ name: 'OBS', value: '#1f222b' }]
		}
	}
};

export default preview;
