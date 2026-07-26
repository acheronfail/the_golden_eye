import { monitor } from '$lib/stores/monitor.svelte';

export const triggerKiaDeathOverlay = (): void => {
	monitor.kiaEffectId += 1;
};
