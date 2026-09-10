<script setup lang="ts">
	import { MdPreview } from 'md-editor-v3';
	import 'md-editor-v3/lib/preview.css';
	import { Loader2 } from 'lucide-vue-next';
	import { computed } from 'vue';
	import { computeThemeClasses } from '@/theme/applyTheme';
	import store from '@/store';
	import sanitizeHtml from '@/utils/sanitizeHtml';
	import type { AgentStep, AgentMessage } from '@/types/agent';

	const props = defineProps<{ message: AgentMessage }>();

	const mdTheme = computed<'dark' | 'light'>(() =>
		computeThemeClasses(
			store.state.theme,
			store.state.colorScheme,
			window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false,
		).dark
			? 'dark'
			: 'light',
	);

	function stepLabel(step: AgentStep): string {
		return `"${step.tool}"` + (step.summary ? ` · ${step.summary}` : '');
	}
</script>

<template>
	<div v-if="props.message.role === 'user'" class="flex justify-end">
		<div class="max-w-[90%] rounded-lg bg-brand/10 px-3 py-2 text-sm text-ink whitespace-pre-wrap">
			{{ props.message.content }}
		</div>
	</div>
	<div v-else class="flex justify-start">
		<div
			v-if="props.message.status === 'pending'"
			class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink"
		>
			<div class="flex items-center gap-2">
				<Loader2 class="h-3.5 w-3.5 animate-spin text-ink-subtle" />
				<span>Thinking…</span>
			</div>
			<ul v-if="props.message.steps.length" class="mt-1 space-y-0.5">
				<li v-for="step in props.message.steps" :key="step.seq" class="text-xs text-ink-subtle">
					{{ stepLabel(step) }}
				</li>
			</ul>
		</div>
		<div
			v-else-if="props.message.status === 'failed'"
			class="max-w-[90%] border-l-4 border-red-500 bg-red-50 px-3 py-2 text-sm text-ink whitespace-pre-wrap dark:bg-red-900/20"
		>
			{{ props.message.content }}
		</div>
		<div v-else class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink">
			<MdPreview
				class="agent-md"
				:editor-id="`agent-msg-${props.message.id}`"
				:model-value="props.message.content"
				:theme="mdTheme"
				preview-theme="default"
				code-theme="atom"
				:sanitize="sanitizeHtml"
			/>
			<details v-if="props.message.steps.length" class="mt-1 text-xs text-ink-subtle">
				<summary class="cursor-pointer select-none">{{ props.message.steps.length }} steps</summary>
				<ul class="mt-1 space-y-0.5">
					<li v-for="step in props.message.steps" :key="step.seq">{{ stepLabel(step) }}</li>
				</ul>
			</details>
		</div>
	</div>
</template>

<style>
	/* md-editor-v3 defaults to a fixed 500px box with its own background; render replies inline */
	.agent-md.md-editor {
		height: auto;
		--md-bk-color: transparent;
		--md-bk-color-outstand: transparent;
	}
	.agent-md .md-editor-preview-wrapper {
		padding: 0;
	}
	.agent-md .md-editor-preview {
		font-size: 0.875rem;
	}
</style>
