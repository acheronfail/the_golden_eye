<script lang="ts">
	import { settings } from '$lib';

	const notificationTemplateTokens = [
		{ value: '{broadcast_url}', description: 'YouTube broadcast URL for the stream.' },
		{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the stream event was handled.' },
		{ value: '{timestamp_local}', description: 'ISO timestamp in local time for when the stream event was handled.' },
		{ value: '{unix_seconds}', description: 'Unix timestamp in seconds, suitable for Discord timestamp markup.' }
	];

	let {
		panelClass,
		labelClass,
		hintClass,
		inputClass,
		textareaClass,
		templateTokenClass
	}: {
		panelClass: string;
		labelClass: string;
		hintClass: string;
		inputClass: string;
		textareaClass: string;
		templateTokenClass: string;
	} = $props();
</script>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.discordNotificationsEnabled}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Enable Discord notifications</span>
	</label>
	<p class={hintClass}>Enable notifications in Discord for streaming, requires a webhook URL.</p>
</section>

{#if settings.discordNotificationsEnabled}
	<section class={panelClass}>
		<label class={labelClass} for="discord-webhook-url">Discord webhook URL</label>
		<input
			id="discord-webhook-url"
			type="url"
			bind:value={settings.discordWebhookUrl}
			placeholder="https://discord.com/api/webhooks/..."
			autocomplete="off"
			spellcheck="false"
			class={inputClass}
		/>
	</section>

	<section class={panelClass}>
		<label class={labelClass} for="streaming-started-message-template">Streaming started message template</label>
		<textarea
			id="streaming-started-message-template"
			rows="3"
			bind:value={settings.streamingStartedMessageTemplate}
			placeholder={settings.defaults.streamingStartedMessageTemplate}
			class={textareaClass}
		></textarea>
		<p class={hintClass}>Available tokens</p>
		<div class="flex flex-wrap gap-2">
			{#each notificationTemplateTokens as token}
				<code class={templateTokenClass} title={token.description} aria-label={`${token.value}: ${token.description}`}
					>{token.value}</code
				>
			{/each}
		</div>
	</section>

	<section class={panelClass}>
		<label class={labelClass} for="streaming-stopped-message-template">Streaming stopped message template</label>
		<textarea
			id="streaming-stopped-message-template"
			rows="3"
			bind:value={settings.streamingStoppedMessageTemplate}
			placeholder={settings.defaults.streamingStoppedMessageTemplate}
			class={textareaClass}
		></textarea>
		<p class={hintClass}>Available tokens</p>
		<div class="flex flex-wrap gap-2">
			{#each notificationTemplateTokens as token}
				<code class={templateTokenClass} title={token.description} aria-label={`${token.value}: ${token.description}`}
					>{token.value}</code
				>
			{/each}
		</div>
	</section>
{/if}
