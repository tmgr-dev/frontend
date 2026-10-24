<script setup lang="ts">
	import type { ActionDecision } from '@/actions/tmgr/agent';
	import MarkdownText from '@/components/general/MarkdownText.vue';
	import type { PendingAction } from '@/types/agent';
	import {
		actionOutcome,
		expiryHint,
		isLongBody,
		openActionIds,
	} from '@/utils/agentConfirmation';
	import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';

	const props = defineProps<{
		actions: PendingAction[];
		decide: (decision: ActionDecision) => Promise<void>;
	}>();

	const now = ref(Date.now());
	const inFlight = ref(false);
	const expanded = ref<Set<number>>(new Set());
	let timer: ReturnType<typeof setInterval> | null = null;
	let boundary: ReturnType<typeof setTimeout> | null = null;

	function clearBoundary(): void {
		if (boundary !== null) clearTimeout(boundary);
		boundary = null;
	}

	function scheduleBoundary(): void {
		clearBoundary();
		const current = Date.now();
		const next = props.actions
			.filter((a) => a.status === 'pending')
			.map((a) => new Date(a.expires_at).getTime())
			.filter((t) => !Number.isNaN(t) && t > current)
			.sort((a, b) => a - b)[0];
		if (next === undefined) return;
		boundary = setTimeout(
			() => {
				now.value = Date.now();
				scheduleBoundary();
			},
			Math.min(next - current + 50, 2147483647),
		);
	}

	onMounted(() => {
		timer = setInterval(() => (now.value = Date.now()), 15000);
		scheduleBoundary();
	});
	onBeforeUnmount(() => {
		if (timer !== null) clearInterval(timer);
		clearBoundary();
	});
	watch(() => props.actions, scheduleBoundary);

	const openIds = computed(() => openActionIds(props.actions, now.value));
	const isOpen = computed(() => openIds.value.length > 0);
	const hint = computed(() => {
		const open = props.actions.filter((a) => openIds.value.includes(a.id));
		if (!open.length) return '';
		const soonest = open
			.map((a) => a.expires_at)
			.sort((a, b) => new Date(a).getTime() - new Date(b).getTime())[0];
		return expiryHint(soonest, now.value);
	});

	async function onDecide(decision: ActionDecision): Promise<void> {
		if (inFlight.value) return;
		now.value = Date.now();
		if (!isOpen.value) return;
		inFlight.value = true;
		try {
			await props.decide(decision);
		} finally {
			inFlight.value = false;
		}
	}

	function toggle(id: number): void {
		const next = new Set(expanded.value);
		if (!next.delete(id)) next.add(id);
		expanded.value = next;
	}

	const toneClass = {
		ok: 'text-green-600 dark:text-green-400',
		error: 'text-red-600 dark:text-red-400',
		muted: 'text-ink-subtle',
	};
</script>

<template>
	<div
		class="mt-2 space-y-2 rounded-md border border-line bg-surface p-2.5"
		data-testid="agent-confirmation-card"
	>
		<div>
			<div class="text-sm font-medium text-ink">Confirm changes</div>
			<div class="text-xs text-ink-subtle">
				TMGR AI wants to make these changes
			</div>
		</div>
		<div
			v-for="action in props.actions"
			:key="action.id"
			class="space-y-1 rounded-md bg-surface-sunken px-2.5 py-2"
		>
			<div class="flex items-center gap-2">
				<span class="text-sm font-medium text-ink">
					{{ action.preview.title }}
				</span>
				<span
					v-if="action.risk === 'high'"
					class="rounded-pill bg-red-100 px-1.5 py-0.5 text-[10px] font-medium uppercase text-red-700 dark:bg-red-900/30 dark:text-red-300"
				>
					High risk
				</span>
			</div>
			<dl v-if="action.preview.fields.length" class="space-y-0.5 text-xs">
				<div
					v-for="(field, index) in action.preview.fields"
					:key="index"
					class="flex gap-2"
				>
					<dt class="shrink-0 text-ink-subtle">{{ field.label }}:</dt>
					<dd class="min-w-0 break-words text-ink">{{ field.value }}</dd>
				</div>
			</dl>
			<div v-if="action.preview.body" class="text-xs text-ink">
				<div
					:class="
						isLongBody(action.preview.body) && !expanded.has(action.id)
							? 'max-h-16 overflow-hidden'
							: 'max-h-64 overflow-y-auto'
					"
				>
					<MarkdownText :content="action.preview.body" untrusted />
				</div>
				<button
					v-if="isLongBody(action.preview.body)"
					type="button"
					class="mt-0.5 text-ink-subtle underline hover:text-ink"
					@click="toggle(action.id)"
				>
					{{ expanded.has(action.id) ? 'Show less' : 'Show all' }}
				</button>
			</div>
			<template v-for="outcome in [actionOutcome(action, now)]" :key="0">
				<div
					v-if="outcome"
					class="text-xs font-medium"
					:class="toneClass[outcome.tone]"
				>
					{{ outcome.text }}
					<a
						v-if="outcome.url"
						:href="outcome.url"
						target="_blank"
						rel="noopener noreferrer"
						class="ml-1 underline"
					>
						Open
					</a>
				</div>
			</template>
		</div>
		<div v-if="isOpen" class="flex flex-wrap items-center gap-2">
			<button
				type="button"
				class="rounded-md bg-brand px-3 py-1 text-xs font-medium text-on-accent hover:opacity-90 disabled:opacity-50"
				:disabled="inFlight || !isOpen"
				@click="onDecide('confirm')"
			>
				Confirm
			</button>
			<button
				type="button"
				class="rounded-md border border-line px-3 py-1 text-xs text-ink hover:bg-surface-hover disabled:opacity-50"
				:disabled="inFlight || !isOpen"
				@click="onDecide('cancel')"
			>
				Cancel
			</button>
			<span v-if="hint" class="text-xs text-ink-subtle">{{ hint }}</span>
		</div>
	</div>
</template>
