export {
	DEFAULT_CLIP_FILENAME_TEMPLATE,
	DEFAULT_MIN_FAILED_RUN_LEN_SECS,
	DEFAULT_POST_RUN_PADDING_SECS,
	DEFAULT_PRE_RUN_PADDING_SECS,
	DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
	DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE,
	DEFAULT_UPDATE_CHECK_INTERVAL,
	settings
} from './settings.svelte';
export type { RecordingOptions, Settings, UpdateCheckInterval } from './settings.svelte';

export const VERSION = import.meta.env.VITE_GE_PLUGIN_VERSION ?? '0.0.0-unknown';
export const IS_DEV = import.meta.env.VITE_GE_PLUGIN_VERSION
	? import.meta.env.VITE_GE_PLUGIN_VERSION.includes('0.0.0-dev')
	: false;
