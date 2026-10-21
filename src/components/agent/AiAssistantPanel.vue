<script setup lang="ts">
	import ChatList from '@/components/agent/ChatList.vue';
	import ContextMeter from '@/components/agent/ContextMeter.vue';
	import MicButton from '@/components/voice/MicButton.vue';
	import { useAgentChat } from '@/composable/useAgentChat';
	import { useCurrentWorkspace } from '@/composable/useCurrentWorkspace';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import store from '@/store';
	import { focusField } from '@/utils/focusTarget';
	import { History, Send, Sparkles, X } from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		nextTick,
		onMounted,
		onUnmounted,
		ref,
		watch,
	} from 'vue';

	const AgentMessage = defineAsyncComponent(
		() => import('@/components/agent/AgentMessage.vue'),
	);

	const {
		state,
		loading,
		error,
		replyTick,
		busy,
		load,
		openConversation,
		forgetRemembered,
		send,
		newChat,
		subscribe,
		unsubscribe,
		stopPolling,
	} = useAgentChat();
	const { currentWorkspaceId } = useCurrentWorkspace();
	const { registerModal, unregisterModal } = useModalEscHandler();

	const draft = ref('');
	const scroller = ref<HTMLDivElement | null>(null);
	const input = ref<HTMLInputElement | null>(null);
	const mic = ref<InstanceType<typeof MicButton> | null>(null);

	// TM-222: the field is disabled while a run is in flight, which makes the browser blur it.
	// Take the cursor back as soon as it is usable again so a follow-up can just be typed.
	const focusInput = () => nextTick(() => focusField(input.value));

	const open = computed(() => store.state.aiPanelOpen);
	const view = ref<'chat' | 'list'>('chat');
	const listQuery = ref('');
	const showArchived = ref(false);
	const listBump = ref(0);
	const listRefreshKey = computed(() => replyTick.value + listBump.value);
	const chatList = ref<InstanceType<typeof ChatList> | null>(null);

	function toggleHistory(): void {
		mic.value?.cancel();
		view.value = view.value === 'list' ? 'chat' : 'list';
	}

	async function openFromList(id: number): Promise<void> {
		view.value = 'chat';
		if (id !== state.value.conversationId) await openConversation(id);
	}

	async function newFromList(): Promise<void> {
		view.value = 'chat';
		await newChat();
	}

	async function onChatRemoved(event: {
		id: number;
		action: 'archive' | 'unarchive' | 'delete';
		nextId: number | null;
	}): Promise<void> {
		if (event.action === 'unarchive') return;
		if (event.id !== state.value.conversationId) return;
		if (event.nextId !== null) {
			await openConversation(event.nextId);
			return;
		}
		if (event.action === 'delete') {
			forgetRemembered();
			if (currentWorkspaceId.value) await load(currentWorkspaceId.value);
		} else {
			await newChat();
		}
		listBump.value++;
	}

	function close(): void {
		store.commit('setAiPanelOpen', false);
	}

	function onEsc(): void {
		if (chatList.value?.cancelRename()) return;
		if (view.value === 'list') {
			view.value = 'chat';
			return;
		}
		close();
	}

	async function submit(): Promise<void> {
		const content = draft.value.trim();
		if (!content || busy.value || loading.value) return;
		if (await send(content)) draft.value = '';
		focusInput();
	}

	watch(busy, (running) => {
		if (!running && open.value) focusInput();
	});

	watch(
		open,
		(isOpen) => {
			if (isOpen) {
				if (currentWorkspaceId.value) load(currentWorkspaceId.value);
				store.commit('pushModalToStack', 'ai-panel');
				registerModal('ai-panel', onEsc);
			} else {
				mic.value?.cancel();
				stopPolling();
				view.value = 'chat';
				listQuery.value = '';
				showArchived.value = false;
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
				if (scroller.value)
					scroller.value.scrollTop = scroller.value.scrollHeight;
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
		class="fixed bottom-[var(--statusbar-h,0px)] right-0 top-[var(--titlebar-h,0px)] z-40 flex w-full max-w-md flex-col border-l border-line bg-surface shadow-xl"
		role="complementary"
		aria-label="AI assistant"
	>
		<header
			class="flex h-12 shrink-0 items-center justify-between border-b border-line px-4"
		>
			<div class="flex items-center gap-2 text-sm font-semibold text-ink">
				<Sparkles class="h-4 w-4" /> Ask AI
			</div>
			<div class="flex items-center gap-1">
				<button
					class="rounded-pill p-1 hover:bg-surface-hover hover:text-ink"
					:class="view === 'list' ? 'text-ink' : 'text-ink-subtle'"
					aria-label="Chat history"
					:aria-pressed="view === 'list'"
					@click="toggleHistory"
				>
					<History class="h-4 w-4" />
				</button>
				<button
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="newFromList"
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
		<ChatList
			v-if="view === 'list' && currentWorkspaceId"
			ref="chatList"
			v-model:query="listQuery"
			v-model:show-archived="showArchived"
			:workspace-id="currentWorkspaceId"
			:current-id="state.conversationId"
			:refresh-key="listRefreshKey"
			@open="openFromList"
			@new-chat="newFromList"
			@removed="onChatRemoved"
		/>
		<div
			v-show="view === 'chat'"
			ref="scroller"
			class="flex-1 space-y-3 overflow-y-auto px-4 py-3"
		>
			<p
				v-if="!state.messages.length && !loading"
				class="text-sm text-ink-subtle"
			>
				Ask about this workspace: what is overdue, how much time went into a
				category this week, what a task is about.
			</p>
			<AgentMessage v-for="m in state.messages" :key="m.id" :message="m" />
		</div>
		<footer v-show="view === 'chat'" class="shrink-0 border-t border-line p-3">
			<div
				class="flex items-center gap-2 rounded-pill border border-line bg-surface-sunken py-1 pl-4 pr-1.5 focus-within:border-line-focus"
			>
				<input
					ref="input"
					v-model="draft"
					data-tour="ai.input"
					class="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-subtle"
					placeholder="Ask about this workspace…"
					:disabled="busy || loading"
					@keydown.enter.prevent="submit"
				/>
				<MicButton
					ref="mic"
					v-model="draft"
					:target="input"
					:disabled="busy || loading"
				/>
				<button
					class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-white disabled:opacity-40"
					:disabled="!draft.trim() || busy || loading"
					aria-label="Send"
					title="Send (Enter)"
					@click="submit"
				>
					<Send class="h-3.5 w-3.5" />
				</button>
			</div>
			<p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
			<ContextMeter
				class="mt-1 text-right"
				:conversation-id="state.conversationId"
				:refresh-key="replyTick"
			/>
		</footer>
	</aside>
</template>
