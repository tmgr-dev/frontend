<script setup lang="ts">
	import { Send, Sparkles, X } from 'lucide-vue-next';
	import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
	import AgentMessage from '@/components/agent/AgentMessage.vue';
	import { useAgentChat } from '@/composable/useAgentChat';
	import { useCurrentWorkspace } from '@/composable/useCurrentWorkspace';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import store from '@/store';

	const { state, loading, error, busy, load, send, newChat, subscribe, unsubscribe } =
		useAgentChat();
	const { currentWorkspaceId } = useCurrentWorkspace();
	const { registerModal, unregisterModal } = useModalEscHandler();

	const draft = ref('');
	const scroller = ref<HTMLDivElement | null>(null);

	const open = computed(() => store.state.aiPanelOpen);

	function close(): void {
		store.commit('setAiPanelOpen', false);
	}

	function submit(): void {
		const content = draft.value.trim();
		if (!content || busy.value) return;
		draft.value = '';
		send(content);
	}

	watch(
		open,
		(isOpen) => {
			if (isOpen) {
				if (currentWorkspaceId.value) load(currentWorkspaceId.value);
				store.commit('pushModalToStack', 'ai-panel');
				registerModal('ai-panel', close);
			} else {
				unregisterModal('ai-panel');
				store.commit('removeModalFromStack', 'ai-panel');
			}
		},
		{ immediate: true },
	);

	watch(currentWorkspaceId, (id) => {
		if (open.value && id) load(id);
	});

	watch(
		() => state.value.messages,
		() =>
			nextTick(() => {
				if (scroller.value) scroller.value.scrollTop = scroller.value.scrollHeight;
			}),
		{ deep: true },
	);

	onMounted(() => {
		if (store.state.user?.id) subscribe(store.state.user.id);
	});

	watch(
		() => store.state.user?.id,
		(id) => {
			if (id) subscribe(id);
		},
	);

	onUnmounted(() => {
		if (open.value) {
			unregisterModal('ai-panel');
			store.commit('removeModalFromStack', 'ai-panel');
		}
		unsubscribe();
	});
</script>

<template>
	<aside
		v-show="open"
		class="fixed inset-y-0 right-0 z-40 flex w-full max-w-md flex-col border-l border-line bg-surface shadow-xl"
		role="complementary"
		aria-label="AI assistant"
	>
		<header class="flex h-12 shrink-0 items-center justify-between border-b border-line px-4">
			<div class="flex items-center gap-2 text-sm font-semibold text-ink">
				<Sparkles class="h-4 w-4" /> Ask AI
			</div>
			<div class="flex items-center gap-1">
				<button
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="newChat"
				>
					New chat
				</button>
				<button
					class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
					aria-label="Close"
					@click="close"
				>
					<X class="h-4 w-4" />
				</button>
			</div>
		</header>
		<div ref="scroller" class="flex-1 space-y-3 overflow-y-auto px-4 py-3">
			<p v-if="!state.messages.length && !loading" class="text-sm text-ink-subtle">
				Ask about this workspace: what is overdue, how much time went into a category this week, what
				a task is about.
			</p>
			<AgentMessage v-for="m in state.messages" :key="m.id" :message="m" />
		</div>
		<footer class="shrink-0 border-t border-line p-3">
			<div
				class="flex items-center gap-2 rounded-pill border border-line bg-surface-sunken pl-4 pr-1.5 py-1 focus-within:border-line-strong"
			>
				<input
					v-model="draft"
					class="min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-ink-subtle outline-none"
					placeholder="Ask about this workspace…"
					:disabled="busy"
					@keydown.enter.prevent="submit"
				/>
				<button
					class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-white disabled:opacity-40"
					:disabled="!draft.trim() || busy"
					aria-label="Send"
					title="Send (Enter)"
					@click="submit"
				>
					<Send class="h-3.5 w-3.5" />
				</button>
			</div>
			<p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
		</footer>
	</aside>
</template>
