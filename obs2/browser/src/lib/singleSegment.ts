import categoriesFile from '../../../single-segment-categories.json';
import type { MonitorRunMode } from './api';

export interface SingleSegmentDefinition {
	id: Exclude<MonitorRunMode, 'clips'>;
	title: string;
	description: string;
	selectDifficulty: boolean;
	difficulties: string[];
	levels: string[];
}

export const SINGLE_SEGMENT_CATEGORIES = categoriesFile.categories as SingleSegmentDefinition[];

export const difficultyId = (name: string): number => {
	switch (name) {
		case 'Agent':
			return 0;
		case 'Secret Agent':
			return 1;
		case '00 Agent':
			return 2;
		default:
			return -1;
	}
};

export const runModePath = (mode: MonitorRunMode): string => {
	switch (mode) {
		case 'anyPercent':
			return 'single-segment/any-percent';
		case 'hundredPercent':
			return 'single-segment/100-percent';
		case 'all60':
			return 'single-segment/all-60';
		case 'clips':
		default:
			return 'monitor';
	}
};

export const routeMode = (route: string): MonitorRunMode => {
	switch (route) {
		case 'any-percent':
			return 'anyPercent';
		case '100-percent':
			return 'hundredPercent';
		case 'all-60':
			return 'all60';
		default:
			return 'clips';
	}
};
