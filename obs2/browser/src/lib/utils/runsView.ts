import type { LevelMatch, RunClip, RunSort } from '$lib/api';
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
		keep: () => void;
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

export type RunFilterKey = keyof RunFilters;

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

export const RUN_SORT_OPTIONS: { value: RunSort; label: string }[] = [
	{ value: 'newest', label: 'Newest first' },
	{ value: 'oldest', label: 'Oldest first' },
	{ value: 'fastest', label: 'Fastest first' },
	{ value: 'slowest', label: 'Slowest first' }
];

export function visibleRunClips(clips: RunClip[], filters: RunFilters, sort: RunSort = 'newest'): RunClip[] {
	const queryTerms = filters.search.trim().toLowerCase().split(/\s+/).filter(Boolean);
	const minTime = timeFilterSeconds(filters.minTime);
	const maxTime = timeFilterSeconds(filters.maxTime);
	const filtered = clips.filter((clip) => {
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
	});
	return sortRunClips(filtered, sort);
}

export function parseRunSort(value: string | null | undefined): RunSort {
	return RUN_SORT_OPTIONS.some((option) => option.value === value) ? (value as RunSort) : 'newest';
}

export function sortRunClips(clips: RunClip[], sort: RunSort): RunClip[] {
	return [...clips].sort((a, b) => compareRunClips(a, b, sort));
}

export function groupRunClips(
	clips: RunClip[],
	sort: RunSort,
	now = new Date()
): { label: string | null; clips: RunClip[] }[] {
	if (sort === 'fastest' || sort === 'slowest') return [{ label: null, clips }];

	const groups = new Map<string, RunClip[]>();
	for (const clip of clips) {
		const label = runDateGroupLabel(clip.metadata.timestamp, now);
		groups.set(label, [...(groups.get(label) ?? []), clip]);
	}
	return [...groups].map(([label, groupedClips]) => ({ label, clips: groupedClips }));
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

function compareRunClips(a: RunClip, b: RunClip, sort: RunSort): number {
	if (sort === 'fastest' || sort === 'slowest') {
		const aTime = clipTimeSeconds(a);
		const bTime = clipTimeSeconds(b);
		if (aTime === null) return bTime === null ? compareNewest(a, b) : 1;
		if (bTime === null) return -1;
		const timeOrder = sort === 'fastest' ? aTime - bTime : bTime - aTime;
		return timeOrder || compareNewest(a, b);
	}
	return sort === 'oldest' ? -compareNewest(a, b) : compareNewest(a, b);
}

function compareNewest(a: RunClip, b: RunClip): number {
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
	return activeRunFilters(filters).map((filter) => filter.label);
}

export function activeRunFilters(filters: RunFilters): { key: RunFilterKey; label: string }[] {
	return (
		[
			{ key: 'search', label: filters.search.trim() ? `search: ${filters.search.trim()}` : '' },
			{ key: 'level', label: filters.level ? `level: ${filters.level}` : '' },
			{ key: 'difficulty', label: filters.difficulty ? `difficulty: ${filters.difficulty}` : '' },
			{ key: 'status', label: filters.status ? `status: ${statusLabel(filters.status)}` : '' },
			{
				key: 'language',
				label: filters.language ? `language: ${romLanguageLabel(filters.language) ?? filters.language}` : ''
			},
			{ key: 'minTime', label: filters.minTime ? `min: ${filters.minTime}` : '' },
			{ key: 'maxTime', label: filters.maxTime ? `max: ${filters.maxTime}` : '' }
		] satisfies { key: RunFilterKey; label: string }[]
	).filter((filter) => filter.label);
}

export function formatDate(value: string): string {
	const date = new Date(value);
	return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function formatRunListDate(value: string, includeDate: boolean): string {
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return includeDate
		? date.toLocaleString(undefined, { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' })
		: date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

export function runDateGroupLabel(value: string, now = new Date()): string {
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return 'Unknown date';
	const dateStart = new Date(date.getFullYear(), date.getMonth(), date.getDate());
	const nowStart = new Date(now.getFullYear(), now.getMonth(), now.getDate());
	const daysAgo = Math.round((nowStart.getTime() - dateStart.getTime()) / 86_400_000);
	if (daysAgo === 0) return 'Today';
	if (daysAgo === 1) return 'Yesterday';
	return date.toLocaleDateString(undefined, {
		day: 'numeric',
		month: 'long',
		year: date.getFullYear() === now.getFullYear() ? undefined : 'numeric'
	});
}

export function formatDuration(seconds?: number | null): string | null {
	if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) return null;
	const rounded = Math.max(0, Math.round(seconds));
	const minutes = Math.floor(rounded / 60);
	const secs = rounded % 60;
	return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

export function formatBytes(bytes?: number | null): string | null {
	if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return null;
	const units = ['B', 'KB', 'MB', 'GB', 'TB'];
	let value = bytes;
	let unitIndex = 0;
	while (value >= 1024 && unitIndex < units.length - 1) {
		value /= 1024;
		unitIndex += 1;
	}
	const maximumFractionDigits = value >= 10 || unitIndex === 0 ? 1 : 2;
	return `${value.toLocaleString(undefined, { maximumFractionDigits })} ${units[unitIndex]}`;
}

export function retentionStateLabel(clip: RunClip): string {
	if (!clip.path) return 'History only';
	const state = clip.retentionState ?? 'kept';
	return state.charAt(0).toUpperCase() + state.slice(1);
}

export function retentionReasonLabel(reason?: string | null): string {
	switch (reason) {
		case 'personalBest':
			return 'Personal best';
		case 'manual':
			return 'Kept manually';
		case 'imported':
			return 'Imported clip';
		case 'historyLimit':
			return 'Recent-history limit';
		case 'deleted':
			return 'Video deleted';
		case 'missing':
			return 'Video missing';
		case 'unreadable':
			return 'Video unreadable';
		case 'recent':
			return 'Recent run';
		case null:
		case undefined:
		case '':
			return 'Not specified';
		default:
			return reason
				.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
				.replace(/[-_]+/g, ' ')
				.replace(/^./, (value) => value.toUpperCase());
	}
}

export function fileRows(clip: RunClip): { label: string; value: string | null | undefined }[] {
	return [
		{ label: 'Run timestamp', value: formatDate(clip.metadata.timestamp) },
		{ label: 'Duration', value: formatDuration(clip.durationSecs) },
		{ label: 'Size', value: clip.path ? formatBytes(clip.sizeBytes) : null },
		{ label: 'Created by', value: clip.metadata.comment },
		{ label: 'Retention state', value: retentionStateLabel(clip) },
		{ label: 'Retention reason', value: retentionReasonLabel(clip.retentionReason) }
	];
}

export function directoryPath(directory: { path: string } | undefined): string {
	return directory?.path ?? 'Not configured';
}
