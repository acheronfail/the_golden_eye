<script lang="ts">
	import KiaDeathOverlay from '$lib/components/KiaDeathOverlay.svelte';

	let { durationMs = 2000 }: { durationMs?: number } = $props();
	let trigger = $state(1);

	const replay = () => {
		trigger += 1;
	};
</script>

<main class="relative flex min-h-screen items-center justify-center overflow-hidden bg-[#080a10] px-6 text-center">
	<div
		class="absolute inset-0 opacity-50"
		style="background: radial-gradient(circle at center, #2d313d 0%, #151821 48%, #050608 100%);"
	></div>
	<div class="relative z-10 grid max-w-lg justify-items-center gap-4">
		<p class="font-mono text-xs tracking-[0.28em] text-(--obs-danger) uppercase">Killed in action</p>
		<h1 class="obs-heading text-4xl font-semibold">Mission failed</h1>
		<p class="obs-muted text-sm">The overlay starts automatically when the story opens.</p>
		<button type="button" class="obs-button obs-button-danger mt-3 px-4 py-2 font-mono text-sm" onclick={replay}>
			Replay KIA animation
		</button>
	</div>
</main>

<KiaDeathOverlay {trigger} {durationMs} />
