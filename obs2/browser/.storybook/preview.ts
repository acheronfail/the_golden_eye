import type { Preview } from '@storybook/sveltekit';
import { MINIMAL_VIEWPORTS } from 'storybook/viewport';
import '../src/routes/layout.css';

const preview: Preview = {
	parameters: {
		a11y: { test: 'todo' },
		backgrounds: { default: 'OBS' },
		viewport: {
			options: {
				obsDock: {
					name: 'OBS side dock',
					styles: { width: '420px', height: '900px' },
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
