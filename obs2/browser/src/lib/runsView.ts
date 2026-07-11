import type { RunClip } from './api';

export interface RunFilters {
	search: string;
	level: string;
	difficulty: string;
	status: string;
	language: string;
	minTime: string;
	maxTime: string;
}

export const EMPTY_RUN_FILTERS: RunFilters = {
	search: '',
	level: '',
	difficulty: '',
	status: '',
	language: '',
	minTime: '',
	maxTime: ''
};

export const LANGUAGE_OPTIONS = [
	{ value: 'en', label: 'en' },
	{ value: 'jp', label: 'jp' }
];

export const STATUS_OPTIONS = [
	{ value: 'failed', label: 'failed' },
	{ value: 'abort', label: 'aborted' },
	{ value: 'complete', label: 'completed' },
	{ value: 'kia', label: 'killed in action' }
];

export const DIFFICULTY_OPTIONS = [
	{ value: 'Agent', label: 'agent' },
	{ value: 'Secret Agent', label: 'secret agent' },
	{ value: '00 Agent', label: '00 agent' },
	{ value: '007', label: '007' }
];

export const LEVEL_OPTIONS = [
	'Dam',
	'Facility',
	'Runway',
	'Surface 1',
	'Bunker 1',
	'Silo',
	'Frigate',
	'Surface 2',
	'Bunker 2',
	'Statue',
	'Archives',
	'Streets',
	'Depot',
	'Train',
	'Jungle',
	'Control',
	'Caverns',
	'Cradle',
	'Aztec',
	'Egypt'
];

export function visibleRunClips(clips: RunClip[], filters: RunFilters): RunClip[] {
	const queryTerms = filters.search.trim().toLowerCase().split(/\s+/).filter(Boolean);
	const minTime = timeFilterSeconds(filters.minTime);
	const maxTime = timeFilterSeconds(filters.maxTime);
	return clips
		.filter((clip) => {
			const time = clipTimeSeconds(clip);
			const searchableText = searchableRunText(clip);
			return (
				(queryTerms.length === 0 || queryTerms.every((term) => searchableText.includes(term))) &&
				(!filters.level || clip.metadata.level === filters.level) &&
				(!filters.difficulty || clip.metadata.difficulty === filters.difficulty) &&
				(!filters.status || normalizeStatus(clip.metadata.status) === filters.status) &&
				(!filters.language || clip.metadata.romLanguage === filters.language) &&
				(minTime === null || (time !== null && time >= minTime)) &&
				(maxTime === null || (time !== null && time <= maxTime))
			);
		})
		.sort(compareRunClips);
}

export function hasActiveRunFilters(filters: RunFilters): boolean {
	return Boolean(
		filters.search.trim() ||
		filters.level ||
		filters.difficulty ||
		filters.status ||
		filters.language ||
		filters.minTime ||
		filters.maxTime
	);
}

export function isRunPreviewVisible(clip: RunClip, previewPath: string | null): boolean {
	return previewPath === clip.path;
}

export function isCompleted(clip: RunClip): boolean {
	return clip.metadata.status === 'complete' || clip.metadata.status === 'completed';
}

export function normalizeStatus(status: string): string {
	return status === 'completed' ? 'complete' : status;
}

export function levelLabel(clip: RunClip): string {
	const level = clip.metadata.level || 'unknown';
	return clip.metadata.levelNumber ? `${clip.metadata.levelNumber}. ${level}` : level;
}

export function romLanguageLabel(lang: string): string | null {
	switch (lang) {
		case 'en':
			return 'EN';
		case 'jp':
			return 'JP';
		case '':
			return null;
		default:
			return `${lang.toUpperCase()}`;
	}
}

export function statusLabel(status: string): string {
	switch (status) {
		case 'completed':
		case 'complete':
			return 'complete';
		case 'failed':
			return 'failed';
		case 'abort':
			return 'aborted';
		case 'kia':
			return 'KIA';
		default:
			return status;
	}
}

export function clipTimeSeconds(clip: RunClip): number | null {
	if (Number.isFinite(clip.metadata.timeSeconds)) return clip.metadata.timeSeconds ?? null;
	return parseRunTimeSeconds(clip.metadata.time);
}

export function parseRunTimeSeconds(value?: string | null): number | null {
	if (!value) return null;
	const trimmed = value.trim();
	const parts = trimmed.split(':').map((part) => Number(part));
	if (parts.length === 1 && Number.isFinite(parts[0])) return Math.max(0, parts[0]);
	if (parts.length !== 2 || parts.some((part) => !Number.isFinite(part))) return null;
	const [minutes, seconds] = parts;
	if (seconds < 0 || seconds >= 60) return null;
	return Math.max(0, minutes * 60 + seconds);
}

function timeFilterSeconds(value: string): number | null {
	const parsed = parseRunTimeSeconds(value);
	return parsed === null ? null : parsed;
}

function searchableRunText(clip: RunClip): string {
	return [
		clip.fileName,
		clip.directory,
		levelLabel(clip),
		clip.metadata.time,
		clip.metadata.difficulty,
		statusLabel(clip.metadata.status),
		clip.metadata.romLanguage,
		clip.metadata.timestamp
	]
		.filter(Boolean)
		.join(' ')
		.toLowerCase();
}

function compareRunClips(a: RunClip, b: RunClip): number {
	return (
		b.metadata.timestamp.localeCompare(a.metadata.timestamp) ||
		(b.modified ?? '').localeCompare(a.modified ?? '') ||
		b.path.localeCompare(a.path)
	);
}
