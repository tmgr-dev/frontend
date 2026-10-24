<script setup lang="ts">
	import type { ActionDecision } from '@/actions/tmgr/agent';
	import AgentConfirmationCard from '@/components/agent/AgentConfirmationCard.vue';
	import AttachedFileList from '@/components/attachments/AttachedFileList.vue';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import type { AgentMessage, AgentQuote, AgentStep } from '@/types/agent';
	import {
		formatMessageFullTime,
		formatMessageStats,
		formatMessageTime,
	} from '@/utils/agentMessageMeta';
	import {
		QUOTE_UNAVAILABLE,
		quoteAuthor,
		sentChip,
	} from '@/utils/agentQuotes';
	import { agentToolLabel } from '@/utils/agentToolLabels';
	import { writeClipboard } from '@/utils/share';
	import { Check, Copy, Loader2, Pin, Reply } from 'lucide-vue-next';
	import { onBeforeUnmount, ref } from 'vue';

	const props = defineProps<{
		message: AgentMessage;
		decide?: (messageId: number, decision: ActionDecision) => Promise<void>;
		isQuoteAvailable?: (messageId: number) => boolean;
	}>();

	const emit = defineEmits<{
		pin: [message: AgentMessage];
		reply: [message: AgentMessage];
		jump: [quote: AgentQuote];
	}>();

	const copied = ref(false);
	let copiedTimer: ReturnType<typeof setTimeout> | null = null;

	onBeforeUnmount(() => {
		if (copiedTimer !== null) clearTimeout(copiedTimer);
	});

	const canAct = () =>
		props.message.status === 'done' && !!props.message.content.trim();

	async function copy(): Promise<void> {
		try {
			await writeClipboard(props.message.content);
		} catch {
			return;
		}
		copied.value = true;
		if (copiedTimer !== null) clearTimeout(copiedTimer);
		copiedTimer = setTimeout(() => (copied.value = false), 1500);
	}

	const timeLabel = () => formatMessageTime(props.message.created_at);
	const timeTitle = () => formatMessageFullTime(props.message.created_at);
	const stats = () => formatMessageStats(props.message);
	const quoteAvailable = (quote: AgentQuote) =>
		props.isQuoteAvailable?.(quote.message_id) ?? true;

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
	<div
		v-if="props.message.role === 'user'"
		:data-message-id="props.message.id"
		class="flex justify-end rounded-lg"
	>
		<div class="max-w-[90%] rounded-lg bg-brand/10 px-3 py-2 text-sm text-ink">
			<div class="mb-0.5 flex items-center gap-1.5 text-[11px] text-ink-subtle">
				<span>You</span>
				<time v-if="timeLabel()" :title="timeTitle()">{{ timeLabel() }}</time>
			</div>
			<ul v-if="props.message.quotes?.length" class="space-y-1 mb-1.5">
				<li v-for="(quote, i) in props.message.quotes" :key="i">
					<button
						v-if="quoteAvailable(quote)"
						class="block w-full rounded border-l-2 border-brand bg-surface px-2 py-1 text-left text-xs text-ink-subtle hover:text-ink"
						@click="emit('jump', quote)"
					>
						<span class="font-medium">{{ sentChip(quote).author }}</span>
						<span v-if="sentChip(quote).time">
							· {{ sentChip(quote).time }}</span
						>
						<span class="block truncate">{{ sentChip(quote).text }}</span>
					</button>
					<div
						v-else
						class="rounded border-l-2 border-line bg-surface px-2 py-1 text-xs text-ink-subtle"
					>
						<span class="font-medium">{{ quoteAuthor(quote.role) }}</span>
						<span class="block truncate">{{ sentChip(quote).text }}</span>
						<span class="italic">{{ QUOTE_UNAVAILABLE }}</span>
					</div>
				</li>
			</ul>
			<MarkdownText data-message-body :content="props.message.content" />
			<AttachedFileList
				v-if="props.message.files?.length"
				class="mt-1.5"
				:files="props.message.files"
			/>
			<div
				v-if="canAct()"
				class="mt-1 flex items-center gap-3 text-xs text-ink-subtle"
			>
				<button
					class="flex items-center gap-1 hover:text-ink"
					:aria-label="copied ? 'Copied' : 'Copy message'"
					@click="copy"
				>
					<Check v-if="copied" class="h-3 w-3" />
					<Copy v-else class="h-3 w-3" />
					{{ copied ? 'Copied' : 'Copy' }}
				</button>
				<button
					class="flex items-center gap-1 hover:text-ink"
					aria-label="Reply to message"
					@click="emit('reply', props.message)"
				>
					<Reply class="h-3 w-3" /> Reply
				</button>
				<button
					class="flex items-center gap-1 hover:text-ink"
					@click="emit('pin', props.message)"
				>
					<Pin class="h-3 w-3" /> Pin
				</button>
			</div>
		</div>
	</div>
	<div
		v-else
		:data-message-id="props.message.id"
		class="flex justify-start rounded-lg"
	>
		<div
			v-if="isThinking()"
			class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink"
		>
			<div class="flex items-center gap-2">
				<Loader2 class="h-3.5 w-3.5 animate-spin text-ink-subtle" />
				<span>Thinking…</span>
			</div>
			<ul v-if="props.message.steps.length" class="space-y-0.5 mt-1">
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
			class="max-w-[90%] border-l-4 border-red-500 bg-red-50 px-3 py-2 text-sm whitespace-pre-wrap text-ink dark:bg-red-900/20"
		>
			{{ props.message.content }}
			<p v-if="stats()" class="mt-1 text-[11px] text-ink-subtle">
				{{ stats() }}
			</p>
		</div>
		<div
			v-else
			class="max-w-[90%] rounded-lg bg-surface-sunken px-3 py-2 text-sm text-ink"
		>
			<div class="mb-0.5 flex items-center gap-1.5 text-[11px] text-ink-subtle">
				<span>AI</span>
				<time v-if="timeLabel()" :title="timeTitle()">{{ timeLabel() }}</time>
			</div>
			<MarkdownText
				v-if="props.message.content"
				data-message-body
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
				<ul class="space-y-0.5 mt-1">
					<li v-for="step in props.message.steps" :key="step.seq">
						{{ stepLabel(step) }}
					</li>
				</ul>
			</details>
			<p v-if="stats()" class="mt-1.5 text-[11px] text-ink-subtle">
				{{ stats() }}
			</p>
			<div
				v-if="canAct()"
				class="mt-1 flex items-center gap-3 text-xs text-ink-subtle"
			>
				<button
					class="flex items-center gap-1 hover:text-ink"
					:aria-label="copied ? 'Copied' : 'Copy message'"
					@click="copy"
				>
					<Check v-if="copied" class="h-3 w-3" />
					<Copy v-else class="h-3 w-3" />
					{{ copied ? 'Copied' : 'Copy' }}
				</button>
				<button
					class="flex items-center gap-1 hover:text-ink"
					aria-label="Reply to message"
					@click="emit('reply', props.message)"
				>
					<Reply class="h-3 w-3" /> Reply
				</button>
				<button
					class="flex items-center gap-1 hover:text-ink"
					@click="emit('pin', props.message)"
				>
					<Pin class="h-3 w-3" /> Pin
				</button>
			</div>
		</div>
	</div>
</template>
