import type { RunClip } from './api';
import type { YoutubeVisibility } from './settings.svelte';

export interface YouTubeUploadPreviewOptions {
	titleTemplate: string;
	descriptionTemplate: string;
	visibility: YoutubeVisibility;
	datetimeLocal?: string;
}

export interface YouTubeUploadPreview {
	title: string;
	description: string;
	visibility: YoutubeVisibility;
	visibilityLabel: string;
}

const dateFromTimestamp = (timestamp: string): Date | null => {
	const date = new Date(timestamp);
	return Number.isNaN(date.getTime()) ? null : date;
};

const clipStem = (clip: RunClip): string => {
	const name = clip.fileName || clip.path.split(/[\\/]/).at(-1) || 'clip';
	const extensionStart = name.lastIndexOf('.');
	return extensionStart > 0 ? name.slice(0, extensionStart) : name || 'clip';
};

const pad = (value: number): string => value.toString().padStart(2, '0');

const timezoneOffset = (date: Date): string => {
	const offsetMinutes = -date.getTimezoneOffset();
	const sign = offsetMinutes >= 0 ? '+' : '-';
	const absolute = Math.abs(offsetMinutes);
	return `${sign}${pad(Math.floor(absolute / 60))}${pad(absolute % 60)}`;
};

const formatIsoLocal = (timestamp: string): string => {
	const date = dateFromTimestamp(timestamp);
	return date
		? `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(
				date.getMinutes()
			)}:${pad(date.getSeconds())}${timezoneOffset(date)}`
		: timestamp;
};

export const formatDatetimeLocal = (timestamp: string, locale?: Intl.LocalesArgument): string => {
	const date = dateFromTimestamp(timestamp);
	return date ? date.toLocaleString(locale) : timestamp;
};

export const datetimeLocalForClip = (clip: RunClip, locale?: Intl.LocalesArgument): string =>
	formatDatetimeLocal(clip.metadata.timestamp, locale);

const visibilityLabel = (visibility: YoutubeVisibility): string => {
	if (visibility === 'public') return 'Public';
	if (visibility === 'private') return 'Private';
	return 'Unlisted';
};

const renderTemplate = (template: string, clip: RunClip, datetimeLocal: string): string => {
	const metadata = clip.metadata;
	return template
		.replaceAll('{obs_replay_name}', clipStem(clip))
		.replaceAll('{mission}', '')
		.replaceAll('{part}', '')
		.replaceAll('{difficulty}', metadata.difficulty ?? '')
		.replaceAll('{level}', metadata.level)
		.replaceAll('{levelNumber}', metadata.levelNumber?.toString() ?? '')
		.replaceAll('{time}', metadata.time ?? '')
		.replaceAll('{status}', metadata.status)
		.replaceAll('{timestamp}', metadata.timestamp)
		.replaceAll('{timestamp_local}', formatIsoLocal(metadata.timestamp))
		.replaceAll('{datetime_local}', datetimeLocal)
		.replaceAll('{plugin_version}', metadata.pluginVersion);
};

export const renderYouTubeUploadPreview = (
	clip: RunClip,
	options: YouTubeUploadPreviewOptions
): YouTubeUploadPreview => {
	const datetimeLocal = options.datetimeLocal?.trim() || formatIsoLocal(clip.metadata.timestamp);
	const title = renderTemplate(options.titleTemplate, clip, datetimeLocal).trim() || clipStem(clip);
	const description = renderTemplate(options.descriptionTemplate, clip, datetimeLocal);
	return {
		title,
		description,
		visibility: options.visibility,
		visibilityLabel: visibilityLabel(options.visibility)
	};
};
