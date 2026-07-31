import { DIFFICULTY_LABELS, type DifficultyNumber, type LevelMatch } from '$lib/api';

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
] as const;

export interface MonitorRunIdentity {
	level: string;
	difficulty: string;
}

const identityFromStart = (match: LevelMatch): MonitorRunIdentity | null => {
	const level = MISSION_LEVELS[match.mission - 1]?.[match.part - 1];
	const difficulty = DIFFICULTY_LABELS[match.difficulty as DifficultyNumber];
	return level && difficulty ? { level, difficulty } : null;
};

export const reconcileMonitorRunIdentity = (
	current: MonitorRunIdentity | null,
	match: LevelMatch | null | undefined
): MonitorRunIdentity | null => {
	if (!match) return null;
	const screen = match.screen.trim().toLowerCase();
	if (screen === 'levels' || screen === 'select') return null;
	if (screen === 'start' || screen === 'opts007') return identityFromStart(match);
	return current;
};

export const monitorRunIdentityLabel = (identity: MonitorRunIdentity | null): string =>
	identity ? `${identity.level} / ${identity.difficulty}` : '- / -';
