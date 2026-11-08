<template>
	<section
		class="space-y-3"
		data-tour="connect.prompt"
		data-testid="starter-prompt"
	>
		<i18n-t
			keypath="onboarding.starterPrompt.ready"
			tag="p"
			class="text-sm text-ink-subtle"
		>
			<template #task>
				<span class="font-medium text-ink">Say hello to TMGR</span>
			</template>
		</i18n-t>
		<p class="text-sm font-medium text-ink">
			{{ $t('onboarding.starterPrompt.paste') }}
		</p>
		<CopyCommand
			:code="prompt"
			:label="$t('onboarding.starterPrompt.label')"
			:emit-event="false"
			wrap
		/>
		<p
			v-if="runLogged"
			class="text-sm font-medium text-status-done-fg"
			role="status"
			data-testid="run-logged"
		>
			{{ $t('onboarding.starterPrompt.runLogged') }}
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
