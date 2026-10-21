<script setup lang="ts">
	import AgentComposerAttachments from '@/components/agent/AgentComposerAttachments.vue';
	import ChatList from '@/components/agent/ChatList.vue';
	import ContextMeter from '@/components/agent/ContextMeter.vue';
	import MicButton from '@/components/voice/MicButton.vue';
	import { useAgentChat } from '@/composable/useAgentChat';
	import { useCurrentWorkspace } from '@/composable/useCurrentWorkspace';
	import { useModalEscHandler } from '@/composable/useModalEscHandler';
	import store from '@/store';
	import { compactionsByAnchor } from '@/utils/agentCompaction';
	import { hasOpenConfirmation } from '@/utils/agentConfirmation';
	import {
		fillScenarios,
		runScenarios,
		type FillScenario,
		type RunScenario,
	} from '@/utils/agentScenarios';
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
		compactions,
		compacting,
		canCompact,
		compact,
		load,
		openConversation,
		forgetRemembered,
		send,
		decide,
		newChat,
		ensureConversation,
		subscribe,
		unsubscribe,
		stopPolling,
	} = useAgentChat();
	const hasOpenCard = computed(() => hasOpenConfirmation(state.value.messages));
	const { currentWorkspaceId } = useCurrentWorkspace();
	const { registerModal, unregisterModal } = useModalEscHandler();

	const draft = ref('');
	const scroller = ref<HTMLDivElement | null>(null);
	const input = ref<HTMLInputElement | null>(null);
	const mic = ref<InstanceType<typeof MicButton> | null>(null);
	const attachments = ref<InstanceType<typeof AgentComposerAttachments> | null>(
		null,
	);
	const dragging = ref(false);

	const hasFiles = (event: DragEvent): boolean =>
		Array.from(event.dataTransfer?.types ?? []).includes('Files');

	function onDragOver(event: DragEvent): void {
		if (!hasFiles(event)) return;
		event.preventDefault();
		dragging.value = true;
	}

	function onDrop(event: DragEvent): void {
		dragging.value = false;
		if (!hasFiles(event)) return;
		event.preventDefault();
		attachments.value?.add(Array.from(event.dataTransfer?.files ?? []));
	}

	function onPaste(event: ClipboardEvent): void {
		const files = Array.from(event.clipboardData?.files ?? []);
		if (!files.length) return;
		event.preventDefault();
		attachments.value?.add(files);
	}

	// TM-222: the field is disabled while a run is in flight, which makes the browser blur it.
	// Take the cursor back as soon as it is usable again so a follow-up can just be typed.
	const focusInput = () => nextTick(() => focusField(input.value));

	const anchors = computed(() => compactionsByAnchor(compactions.value));
	const compactionsAfter = (messageId: number) =>
		anchors.value.get(messageId) ?? [];

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
		if (!attachments.value?.canSend) return;
		const sent = await send(
			content,
			undefined,
			attachments.value.fileIds,
			attachments.value.readyFiles,
		);
		if (sent) {
			draft.value = '';
			attachments.value.clear();
		}
		focusInput();
	}

	async function runScenario(scenario: RunScenario): Promise<void> {
		if (busy.value || loading.value) return;
		await send(scenario.label, scenario.id);
		focusInput();
	}

	function fillScenario(scenario: FillScenario): void {
		if (busy.value || loading.value) return;
		draft.value = scenario.template;
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
		mic.value?.cancel();
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
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink disabled:pointer-events-none disabled:opacity-40"
					:disabled="!canCompact"
					:title="
						canCompact || compacting ? undefined : 'Available between answers'
					"
					@click="compact"
				>
					{{ compacting ? 'Compacting…' : 'Compact history' }}
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
			<template v-for="m in state.messages" :key="m.id">
				<AgentMessage :message="m" :decide="decide" />
				<div
					v-for="c in compactionsAfter(m.id)"
					:key="`compaction-${c.id}`"
					class="flex items-center gap-2 text-xs text-ink-subtle"
				>
					<span class="h-px flex-1 bg-line"></span>
					<span
						>History compacted{{
							c.trigger === 'auto' ? ' (automatic)' : ''
						}}</span
					>
					<span class="h-px flex-1 bg-line"></span>
				</div>
			</template>
		</div>
		<footer
			v-show="view === 'chat'"
			class="shrink-0 border-t border-line p-3"
			@dragover="onDragOver"
			@dragleave="dragging = false"
			@drop="onDrop"
		>
			<AgentComposerAttachments
				ref="attachments"
				:conversation-id="state.conversationId"
				:workspace-id="state.workspaceId"
				:ensure-conversation="ensureConversation"
				:disabled="busy || loading"
			/>
			<div
				v-if="!busy && !loading"
				class="mb-2 flex flex-wrap gap-1.5"
				data-tour="ai.scenarios"
			>
				<button
					v-for="scenario in runScenarios"
					:key="scenario.id"
					class="rounded-pill border border-line px-2.5 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="runScenario(scenario)"
				>
					{{ scenario.label }}
				</button>
				<button
					v-for="scenario in fillScenarios"
					:key="scenario.label"
					class="rounded-pill border border-dashed border-line px-2.5 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="fillScenario(scenario)"
				>
					{{ scenario.label }}
				</button>
			</div>
			<div
				class="flex items-center gap-2 rounded-pill border border-line bg-surface-sunken py-1 pl-4 pr-1.5 focus-within:border-line-focus"
				:class="
					dragging
						? 'ring-2 ring-brand ring-offset-1 ring-offset-surface dark:ring-offset-gray-900'
						: ''
				"
			>
				<input
					ref="input"
					v-model="draft"
					data-tour="ai.input"
					class="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-subtle"
					placeholder="Ask about this workspace…"
					:disabled="busy || loading"
					@keydown.enter.prevent="submit"
					@paste="onPaste"
				/>
				<MicButton
					ref="mic"
					v-model="draft"
					:target="input"
					:disabled="busy || loading"
				/>
				<button
					class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-white disabled:opacity-40"
					:disabled="!draft.trim() || busy || loading || !attachments?.canSend"
					aria-label="Send"
					title="Send (Enter)"
					@click="submit"
				>
					<Send class="h-3.5 w-3.5" />
				</button>
			</div>
			<p v-if="hasOpenCard" class="mt-1 text-xs text-ink-subtle">
				Sending a new message will cancel the pending changes.
			</p>
			<p v-if="error" class="mt-1 text-xs text-red-500">{{ error }}</p>
			<ContextMeter
				class="mt-1 text-right"
				:conversation-id="state.conversationId"
				:refresh-key="replyTick"
			/>
		</footer>
	</aside>
</template>
