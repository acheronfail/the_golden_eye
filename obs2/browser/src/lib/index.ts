export { settings } from './settings.svelte';
export type { RecordingOptions, Settings, UpdateCheckInterval } from './settings.svelte';
export { default as Select } from './Select.svelte';
export type { SelectOption } from './Select.svelte';

export const VERSION = import.meta.env.VITE_GE_PLUGIN_VERSION ?? '0.0.0-unknown';
export const IS_DEV = import.meta.env.VITE_GE_PLUGIN_VERSION
	? import.meta.env.VITE_GE_PLUGIN_VERSION.includes('0.0.0-dev')
	: false;
