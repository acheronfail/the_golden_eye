import type { LevelMatch, RunClip } from '$lib/api';
import type { MetaPill } from './metaPills';
import type { SelectOption } from '$lib/components/Select.svelte';

export interface RunDetailView {
	modal: {
		error: string | null;
		busy: string | null;
	};
	display: {
		fileBrowserLabel: string;
		levelOptions: SelectOption[];
	};
	actions: {
		close: () => void;
		delete: () => void;
		reveal: () => void;
		rename: () => void;
		saveMetadata: () => void;
		normalizeDraftTime: () => void;
	};
}

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
export function runDetail(clip: RunClip): string {
	const parts = [
		levelLabel(clip),
		romLanguageLabel(clip.metadata.romLanguage),
		clip.metadata.time,
		clip.metadata.difficulty,
		statusLabel(clip.metadata.status),
		formatDate(clip.metadata.timestamp)
	];
	return parts.filter(Boolean).join(' | ');
}

export function statusTone(status: string): string {
	switch (status === 'completed' ? 'complete' : status) {
		case 'complete':
			return 'border-[color-mix(in_srgb,var(--obs-success),var(--obs-border)_35%)] bg-(--obs-success-surface) text-(--obs-success)';
		case 'failed':
		case 'abort':
		case 'kia':
			return 'border-[color-mix(in_srgb,var(--obs-danger),var(--obs-border)_35%)] bg-(--obs-danger-surface) text-(--obs-danger)';
		default:
			return 'obs-token';
	}
}

const MISSION_LEVELS = [
	['Dam', 'Facility', 'Runway'],
	['Surface 1', 'Bunker 1'],
	['Silo'],
	['Frigate'],
	['Surface 2', 'Bunker 2'],
	['Statue', 'Archives', 'Streets', 'Depot', 'Train'],
	['Jungle', 'Control', 'Caverns', 'Cradle'],
	['Aztec'],
	['Egypt']
];
const DIFFICULTY_LABELS = ['Agent', 'Secret Agent', '00 Agent', '007'];

function levelMatchInfo(match?: LevelMatch): { name: string; number?: number } {
	if (!match) return { name: 'unknown' };
	const missionIndex = match.mission - 1;
	const partIndex = match.part - 1;
	const name = MISSION_LEVELS[missionIndex]?.[partIndex] ?? 'unknown';
	if (name === 'unknown') return { name };
	const previousLevelCount = MISSION_LEVELS.slice(0, missionIndex).reduce((total, levels) => total + levels.length, 0);
	return { name, number: previousLevelCount + partIndex + 1 };
}

function levelMatchLabel(match?: LevelMatch): string {
	const level = levelMatchInfo(match);
	return level.number ? `${level.number}. ${level.name}` : level.name;
}

function difficultyLabel(value?: number): string | null {
	return value === undefined ? null : (DIFFICULTY_LABELS[value] ?? null);
}

export function runMetaChips(clip: RunClip): MetaPill[] {
	return [
		{ label: levelLabel(clip), class: 'obs-token' },
		{ label: clip.metadata.time ?? '', class: 'obs-token' },
		{ label: clip.metadata.difficulty ?? '', class: 'obs-token' },
		{ label: romLanguageLabel(clip.metadata.romLanguage) ?? '', class: 'obs-token' },
		{ label: statusLabel(clip.metadata.status), class: statusTone(clip.metadata.status) }
	].filter((chip) => Boolean(chip.label));
}

export function levelMatchMetaChips(
	match: LevelMatch | undefined,
	options: { failed?: boolean; durationSecs?: number } = {}
): MetaPill[] {
	const status = options.failed ? 'failed' : 'complete';
	const duration =
		options.durationSecs === undefined
			? ''
			: (formatDuration(options.durationSecs) ?? `${options.durationSecs.toFixed(1)}s`);
	if (!match) {
		return (
			[
				{ label: duration, class: 'obs-token' },
				{ label: statusLabel(status), class: statusTone(status) }
			] satisfies MetaPill[]
		).filter((chip) => Boolean(chip.label));
	}

	const time = match.times?.time;
	return [
		{ label: levelMatchLabel(match), class: 'obs-token' },
		{ label: time === undefined ? '' : (formatDuration(time) ?? ''), class: 'obs-token' },
		{ label: difficultyLabel(match.difficulty) ?? '', class: 'obs-token' },
		{ label: romLanguageLabel(match.detected_lang ?? '') ?? '', class: 'obs-token' },
		{ label: statusLabel(status), class: statusTone(status) }
	].filter((chip) => Boolean(chip.label));
}

export function activeRunFilterLabels(filters: RunFilters): string[] {
	return [
		filters.search.trim() ? `search: ${filters.search.trim()}` : '',
		filters.level ? `level: ${filters.level}` : '',
		filters.difficulty ? `difficulty: ${filters.difficulty}` : '',
		filters.status ? `status: ${statusLabel(filters.status)}` : '',
		filters.language ? `language: ${romLanguageLabel(filters.language) ?? filters.language}` : '',
		filters.minTime ? `min: ${filters.minTime}` : '',
		filters.maxTime ? `max: ${filters.maxTime}` : ''
	].filter((label) => label);
}

export function formatDate(value: string): string {
	const date = new Date(value);
	return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function formatDuration(seconds?: number | null): string | null {
	if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return null;
	const rounded = Math.max(0, Math.round(seconds));
	const minutes = Math.floor(rounded / 60);
	const secs = rounded % 60;
	return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

export function fileRows(clip: RunClip): { label: string; value: string | null | undefined }[] {
	return [
		{ label: 'Run timestamp', value: formatDate(clip.metadata.timestamp) },
		{ label: 'Duration', value: formatDuration(clip.durationSecs) },
		{ label: 'Created by', value: clip.metadata.comment }
	];
}

export function directoryPath(directory: { path: string } | undefined): string {
	return directory?.path ?? 'Not configured';
}
