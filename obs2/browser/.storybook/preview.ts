import type { Preview } from '@storybook/sveltekit';
import '../src/routes/layout.css';

const preview: Preview = {
	parameters: {
		a11y: { test: 'todo' },
		backgrounds: { default: 'OBS' },
		controls: {
			matchers: {
				color: /(background|color)$/i,
				date: /Date$/i
			}
		}
	},
	globalTypes: {
		backgrounds: {
			defaultValue: 'OBS',
			items: [{ name: 'OBS', value: '#1f222b' }]
		}
	}
};

export default preview;
