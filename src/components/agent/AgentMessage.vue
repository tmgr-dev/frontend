<script setup lang="ts">
	import type { ActionDecision } from '@/actions/tmgr/agent';
	import AgentConfirmationCard from '@/components/agent/AgentConfirmationCard.vue';
	import AttachedFileList from '@/components/attachments/AttachedFileList.vue';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import type { AgentMessage, AgentStep } from '@/types/agent';
	import { agentToolLabel } from '@/utils/agentToolLabels';
	import { Loader2 } from 'lucide-vue-next';

	const props = defineProps<{
		message: AgentMessage;
		decide?: (messageId: number, decision: ActionDecision) => Promise<void>;
	}>();

	const isThinking = () =>
		props.message.status === 'pending' || props.message.status === 'running';

	const onDecide = (d: ActionDecision) =>
		props.decide?.(props.message.id, d) ?? Promise.resolve();

	const hasCard = () => !!props.message.pending_actions?.length;

	function stepLabel(step: AgentStep): string {
		return (
			agentToolLabel(step.tool) + (step.summary ? ` · ${step.summary}` : '')
		);
	}
</script>

<template>
	<div v-if="props.message.role === 'user'" class="flex justify-end">
		<div class="bg-brand/10 max-w-[90%] rounded-lg px-3 py-2 text-sm text-ink">
			<MarkdownText :content="props.message.content" />
			<AttachedFileList
				v-if="props.message.files?.length"
				class="mt-1.5"
				:files="props.message.files"
			/>
		</div>
	</div>
	<div v-else class="flex justify-start">
		<div
			v-if="isThinking()"
			class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink"
		>
			<div class="flex items-center gap-2">
				<Loader2 class="h-3.5 w-3.5 animate-spin text-ink-subtle" />
				<span>Thinking…</span>
			</div>
			<ul v-if="props.message.steps.length" class="mt-1 space-y-0.5">
				<li
					v-for="step in props.message.steps"
					:key="step.seq"
					class="text-xs text-ink-subtle"
				>
					{{ stepLabel(step) }}
				</li>
			</ul>
			<AgentConfirmationCard
				v-if="hasCard()"
				:actions="props.message.pending_actions ?? []"
				:decide="onDecide"
			/>
		</div>
		<div
			v-else-if="props.message.status === 'failed'"
			class="max-w-[90%] whitespace-pre-wrap border-l-4 border-red-500 bg-red-50 px-3 py-2 text-sm text-ink dark:bg-red-900/20"
		>
			{{ props.message.content }}
		</div>
		<div
			v-else
			class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink"
		>
			<MarkdownText
				v-if="props.message.content"
				:content="props.message.content"
				untrusted
			/>
			<AgentConfirmationCard
				v-if="hasCard()"
				:actions="props.message.pending_actions ?? []"
				:decide="onDecide"
			/>
			<details
				v-if="props.message.steps.length"
				class="mt-1 text-xs text-ink-subtle"
			>
				<summary class="cursor-pointer select-none">
					{{ props.message.steps.length }} steps
				</summary>
				<ul class="mt-1 space-y-0.5">
					<li v-for="step in props.message.steps" :key="step.seq">
						{{ stepLabel(step) }}
					</li>
				</ul>
			</details>
		</div>
	</div>
</template>
