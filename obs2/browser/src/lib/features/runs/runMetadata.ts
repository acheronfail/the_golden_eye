import type { EditableRunMetadata, RunClip } from '$lib/api';
import {
	DIFFICULTY_OPTIONS,
	LANGUAGE_OPTIONS,
	LEVEL_OPTIONS,
	ROM_VERSION_OPTIONS,
	STATUS_OPTIONS
} from '$lib/features/runs/runsView';

function hasValue<T extends string>(options: readonly { value: T }[], value: string | undefined | null): value is T {
	return Boolean(value && options.some((option) => option.value === value));
}

export function metadataDraftFromClip(clip: RunClip): EditableRunMetadata {
	return {
		gameLanguage: hasValue(LANGUAGE_OPTIONS, clip.metadata.gameLanguage) ? clip.metadata.gameLanguage : '',
		romVersion: hasValue(ROM_VERSION_OPTIONS, clip.metadata.romVersion) ? clip.metadata.romVersion : '',
		status: hasValue(STATUS_OPTIONS, clip.metadata.status) ? clip.metadata.status : '',
		difficulty: hasValue(DIFFICULTY_OPTIONS, clip.metadata.difficulty) ? clip.metadata.difficulty : '',
		time: clip.metadata.time ?? '',
		level: clip.metadata.level && LEVEL_OPTIONS.includes(clip.metadata.level) ? clip.metadata.level : ''
	};
}

export function sameMetadataDraft(a: EditableRunMetadata, b: EditableRunMetadata): boolean {
	return (
		a.gameLanguage === b.gameLanguage &&
		a.romVersion === b.romVersion &&
		a.status === b.status &&
		a.difficulty === b.difficulty &&
		a.time === b.time &&
		a.level === b.level
	);
}

export function normalizeRunTimeInput(value: string): string {
	const trimmed = value.trim();
	if (!trimmed) return '';
	const [minutes, seconds, extra] = trimmed.split(':');
	if (extra !== undefined || !minutes || seconds === undefined) return trimmed;
	if (!/^\d+$/.test(minutes) || !/^\d{1,2}$/.test(seconds)) return trimmed;
	const minuteValue = Number(minutes);
	const secondValue = Number(seconds);
	if (!Number.isInteger(minuteValue) || !Number.isInteger(secondValue) || secondValue > 59) return trimmed;
	return `${minuteValue.toString().padStart(2, '0')}:${secondValue.toString().padStart(2, '0')}`;
}

export function runBrowserLabels(platform: string): { file: string; folder: string } {
	const normalized = platform.toLowerCase();
	if (normalized.includes('mac')) return { file: 'Show in Finder', folder: 'show clips in finder' };
	if (normalized.includes('win')) return { file: 'Show in Explorer', folder: 'show clips in explorer' };
	return { file: 'Show in file browser', folder: 'show clips folder' };
}
