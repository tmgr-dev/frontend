<template>
	<section class="space-y-3" data-tour="connect.prompt" data-testid="starter-prompt">
		<p class="text-sm text-ink-subtle">
			Your first task is ready: <span class="font-medium text-ink">Say hello to TMGR</span>
		</p>
		<p class="text-sm font-medium text-ink">Paste this into your agent:</p>
		<CopyCommand :code="prompt" label="Starter prompt" :emit-event="false" wrap />
		<p
			v-if="runLogged"
			class="text-sm font-medium text-green-700 dark:text-green-400"
			role="status"
			data-testid="run-logged"
		>
			Run logged. TMGR recorded what your agent did.
		</p>
	</section>
</template>

<script lang="ts">
import { defineComponent } from 'vue';
import CopyCommand from './CopyCommand.vue';

const STARTER_PROMPT =
	'Use the tmgr MCP server: find my task "Say hello to TMGR", start agent work on it, add a hello comment, then finish the agent work.';

export default defineComponent({
	name: 'StarterPrompt',
	components: { CopyCommand },
	props: {
		runLogged: { type: Boolean, default: false },
	},
	setup() {
		return { prompt: STARTER_PROMPT };
	},
});
</script>
