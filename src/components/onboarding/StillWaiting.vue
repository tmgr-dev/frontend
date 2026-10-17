<template>
	<section
		class="rounded-card border border-line bg-surface-sunken p-4 text-sm text-ink dark:border-gray-700 dark:bg-gray-950"
		aria-labelledby="still-waiting-title"
		data-testid="still-waiting"
	>
		<h2 id="still-waiting-title" class="text-base font-semibold">
			Still waiting for your agent
		</h2>
		<p class="mt-1 text-ink-subtle">
			No call has reached TMGR yet. Check these:
		</p>
		<ol class="mt-3 list-decimal space-y-2 pl-5">
			<li>
				Run the command in the terminal where you start {{ client }}, not inside
				a session.
			</li>
			<li>Restart the agent so it reloads its MCP config.</li>
			<li v-if="verifyCommand">
				Run <CommandChip :command="verifyCommand" /> and look for tmgr.
			</li>
		</ol>
		<div class="mt-4 flex flex-wrap gap-2">
			<button
				type="button"
				class="min-h-[44px] rounded-md bg-brand px-4 text-sm font-medium text-white hover:opacity-90 sm:min-h-[36px]"
				data-tour-avoid="cta"
				@click="$emit('check-again')"
			>
				Check again
			</button>
			<button
				type="button"
				class="min-h-[44px] rounded-md border border-line px-4 text-sm font-medium text-ink hover:bg-surface dark:border-gray-700 sm:min-h-[36px]"
				@click="$emit('different-client')"
			>
				Use a different client
			</button>
		</div>
		<div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1">
			<router-link
				:to="{ name: 'AgentsGuide' }"
				class="inline-flex min-h-[44px] items-center text-ink-subtle underline hover:text-ink sm:min-h-0"
			>
				Open troubleshooting guide
			</router-link>
			<button
				type="button"
				class="inline-flex min-h-[44px] items-center text-ink-subtle underline hover:text-ink sm:min-h-0"
				data-tour-avoid="cta"
				@click="$emit('later')"
			>
				Do this later
			</button>
		</div>
	</section>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'StillWaiting',
		components: { CommandChip },
		props: {
			client: { type: String, default: 'your agent' },
			verifyCommand: { type: String, default: '' },
		},
		emits: ['check-again', 'different-client', 'later'],
	});
</script>
