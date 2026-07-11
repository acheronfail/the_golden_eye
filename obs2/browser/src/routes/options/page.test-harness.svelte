<script lang="ts">
	import Layout from '../+layout.svelte';
	import OptionsPage from './+page.svelte';
</script>

<Layout>
	{#snippet children()}
		<OptionsPage />
	{/snippet}
</Layout>
