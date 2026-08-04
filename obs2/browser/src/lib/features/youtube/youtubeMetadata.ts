import type { RunClip } from '$lib/api';
import type { YoutubeVisibility } from '$lib/stores/settings.svelte';
import { romVersionLabel } from '$lib/features/runs/runsView';
import { missionAndPartFromLevel, NumberLevelMap } from './geLevels';

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

export const formatDatetimeLocal = (timestamp: string): string => {
	const date = dateFromTimestamp(timestamp);
	return date ? date.toLocaleString(typeof navigator === 'undefined' ? undefined : navigator.languages) : timestamp;
};

export const datetimeLocalForClip = (clip: RunClip): string => formatDatetimeLocal(clip.metadata.timestamp);

const visibilityLabel = (visibility: YoutubeVisibility): string => {
	if (visibility === 'public') return 'Public';
	if (visibility === 'private') return 'Private';
	return 'Unlisted';
};

const partToRomanNumeral = (n: number) => {
	switch (n) {
		case 1:
			return 'i';
		case 2:
			return 'ii';
		case 3:
			return 'iii';
		case 4:
			return 'iv';
		case 5:
			return 'v';
		default:
			return '';
	}
};

const renderTemplate = (template: string, clip: RunClip, datetimeLocal: string): string => {
	const metadata = clip.metadata;

	// NOTE: first try from name and then level number, same as `RunTemplateTokens::from_clip_metadata`
	let missionAndPart = missionAndPartFromLevel(metadata.level);
	if (!missionAndPart && typeof metadata.levelNumber === 'number') {
		missionAndPart = missionAndPartFromLevel(NumberLevelMap.get(metadata.levelNumber));
	}
	let mission = missionAndPart ? missionAndPart[0].toString() : '';
	let part = missionAndPart ? partToRomanNumeral(missionAndPart[1]) : '';

	return template
		.replaceAll('{obs_replay_name}', clipStem(clip))
		.replaceAll('{mission}', mission)
		.replaceAll('{part}', part)
		.replaceAll('{difficulty}', metadata.difficulty ?? '')
		.replaceAll('{level}', metadata.level)
		.replaceAll('{levelNumber}', metadata.levelNumber?.toString() ?? '')
		.replaceAll('{time}', metadata.time ?? '')
		.replaceAll('{status}', metadata.status)
		.replaceAll('{rom}', romVersionLabel(metadata.romVersion) ?? 'unknown')
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
