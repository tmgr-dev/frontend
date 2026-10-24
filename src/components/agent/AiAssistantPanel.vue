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
	import { daySeparators } from '@/utils/agentMessageMeta';
	import {
		canAddPin,
		canEditPin,
		pinLimitHint,
		pinTextFromMessage,
	} from '@/utils/agentPins';
	import { flashQuote, selectedQuote } from '@/utils/agentQuoteDom';
	import { draftChip } from '@/utils/agentQuotes';
	import {
		fillScenarios,
		runScenarios,
		type FillScenario,
		type RunScenario,
	} from '@/utils/agentScenarios';
	import { focusField } from '@/utils/focusTarget';
	import { History, Pin, Quote, Send, Sparkles, X } from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		nextTick,
		onMounted,
		onUnmounted,
		ref,
		watch,
	} from 'vue';

	import type {
		AgentPin,
		AgentQuote,
		AgentMessage as ChatMessage,
	} from '@/types/agent';

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
		pins,
		pinLimits,
		pinsLoading,
		pinSaving,
		pinError,
		quotes,
		addQuoteDraft,
		removeQuoteDraft,
		addPin,
		editPin,
		removePin,
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

	const pinsOpen = ref(false);
	const pinDraft = ref('');
	const pinSourceId = ref<number | null>(null);
	const pinInput = ref<HTMLTextAreaElement | null>(null);
	const editingPinId = ref<number | null>(null);
	const editDraft = ref('');
	const canSubmitPin = computed(() =>
		canAddPin(pins.value, pinLimits.value, pinDraft.value),
	);
	const pinHint = computed(() =>
		pinLimitHint(pins.value, pinLimits.value, pinDraft.value, null),
	);
	const editHint = computed(() =>
		editingPinId.value === null
			? ''
			: pinLimitHint(
					pins.value,
					pinLimits.value,
					editDraft.value,
					editingPinId.value,
				),
	);

	const quoteChips = computed(() =>
		quotes.value.map((draft) => draftChip(draft, state.value.messages)),
	);
	const separators = computed(() => daySeparators(state.value.messages));

	const isQuoteAvailable = (messageId: number) =>
		state.value.messages.some((m) => m.id === messageId);

	function replyTo(message: ChatMessage): void {
		addQuoteDraft({ message_id: message.id, text: null });
		focusInput();
	}

	async function jumpToQuote(quote: AgentQuote): Promise<void> {
		if (!isQuoteAvailable(quote.message_id)) return;
		for (let attempt = 0; attempt < 10; attempt++) {
			await nextTick();
			const el = scroller.value?.querySelector<HTMLElement>(
				`[data-message-id="${quote.message_id}"]`,
			);
			if (el) {
				el.scrollIntoView({ block: 'center', behavior: 'smooth' });
				flashQuote(el, quote.whole ? null : quote.text);
				return;
			}
			await new Promise((resolve) => setTimeout(resolve, 50));
		}
	}

	const quoteButton = ref<{
		messageId: number;
		text: string;
		top: number;
		left: number;
	} | null>(null);
	let pointerDown = false;

	function refreshQuoteButton(): void {
		const picked =
			open.value && view.value === 'chat'
				? selectedQuote(scroller.value)
				: null;
		if (!picked) {
			quoteButton.value = null;
			return;
		}
		const { rect } = picked;
		quoteButton.value = {
			messageId: picked.messageId,
			text: picked.text,
			top: rect.top > 44 ? rect.top - 36 : rect.bottom + 6,
			left: Math.min(
				Math.max(rect.left + rect.width / 2, 48),
				window.innerWidth - 48,
			),
		};
	}

	function onSelectionChange(): void {
		if (!pointerDown) refreshQuoteButton();
		else if (!selectedQuote(scroller.value)) quoteButton.value = null;
	}

	function onPointerDown(event: MouseEvent): void {
		if ((event.target as Element).closest('[data-quote-button]')) return;
		pointerDown = true;
	}

	function onPointerUp(): void {
		if (!pointerDown) return;
		pointerDown = false;
		setTimeout(refreshQuoteButton, 0);
	}

	function quoteSelection(): void {
		const picked = quoteButton.value;
		if (!picked) return;
		addQuoteDraft({ message_id: picked.messageId, text: picked.text });
		window.getSelection()?.removeAllRanges();
		quoteButton.value = null;
		focusInput();
	}

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

	function togglePins(): void {
		view.value = 'chat';
		pinsOpen.value = !pinsOpen.value;
		pinSourceId.value = null;
		editingPinId.value = null;
	}

	function pinMessage(message: ChatMessage): void {
		view.value = 'chat';
		pinsOpen.value = true;
		editingPinId.value = null;
		pinSourceId.value = message.id;
		pinDraft.value = pinTextFromMessage(
			message.content,
			pinLimits.value.max_chars,
		);
		nextTick(() => focusField(pinInput.value));
	}

	async function submitPin(): Promise<void> {
		const saved = await addPin(pinDraft.value, pinSourceId.value ?? undefined);
		if (!saved) return;
		pinDraft.value = '';
		pinSourceId.value = null;
	}

	function beginEditPin(pin: AgentPin): void {
		editingPinId.value = pin.id;
		editDraft.value = pin.content;
	}

	async function saveEditedPin(): Promise<void> {
		if (editingPinId.value === null) return;
		const saved = await editPin(editingPinId.value, editDraft.value);
		if (saved) editingPinId.value = null;
	}

	async function deletePin(pin: AgentPin): Promise<void> {
		if (editingPinId.value === pin.id) editingPinId.value = null;
		await removePin(pin.id);
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
		if (pinsOpen.value) {
			pinsOpen.value = false;
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
				pinsOpen.value = false;
				editingPinId.value = null;
				listQuery.value = '';
				showArchived.value = false;
				unregisterModal('ai-panel');
				store.commit('removeModalFromStack', 'ai-panel');
			}
		},
		{ immediate: true },
	);

	watch(
		() => state.value.conversationId,
		() => {
			pinSourceId.value = null;
			editingPinId.value = null;
		},
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
		document.addEventListener('selectionchange', onSelectionChange);
		document.addEventListener('mousedown', onPointerDown);
		document.addEventListener('mouseup', onPointerUp);
	});

	watch(
		() => store.state.user?.id,
		(id) => {
			if (id) subscribe(id);
		},
	);

	onUnmounted(() => {
		document.removeEventListener('selectionchange', onSelectionChange);
		document.removeEventListener('mousedown', onPointerDown);
		document.removeEventListener('mouseup', onPointerUp);
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
					class="relative rounded-pill p-1 hover:bg-surface-hover hover:text-ink"
					:class="pinsOpen ? 'text-ink' : 'text-ink-subtle'"
					:aria-label="`Pinned facts (${pins.length})`"
					:aria-pressed="pinsOpen"
					:title="`Pinned facts (${pins.length})`"
					@click="togglePins"
				>
					<Pin class="h-4 w-4" />
					<span
						v-if="pins.length"
						class="absolute -right-1 -top-1 rounded-pill bg-brand px-1 text-[10px] leading-4 text-on-accent"
						>{{ pins.length }}</span
					>
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
		<section
			v-if="pinsOpen && view === 'chat'"
			class="max-h-80 shrink-0 space-y-3 overflow-y-auto border-b border-line bg-surface px-4 py-3"
			aria-label="Pinned facts"
		>
			<p class="text-xs text-ink-subtle">
				Pinned facts are sent to the assistant with every message and are kept
				when the history is compacted.
			</p>
			<p v-if="pinsLoading && !pins.length" class="text-sm text-ink-subtle">
				Loading…
			</p>
			<p v-else-if="!pins.length" class="text-sm text-ink-subtle">
				No pinned facts yet.
			</p>
			<ul v-else class="space-y-2">
				<li
					v-for="pin in pins"
					:key="pin.id"
					class="rounded-lg border border-line bg-surface-sunken p-2 text-sm text-ink"
				>
					<template v-if="editingPinId === pin.id">
						<textarea
							v-model="editDraft"
							rows="3"
							:maxlength="pinLimits.max_chars"
							aria-label="Edit pinned fact"
							class="w-full resize-none rounded-lg border border-line bg-surface px-2 py-1.5 text-sm text-ink outline-hidden focus:border-line-focus"
						></textarea>
						<div class="mt-1 flex items-center justify-between gap-2 text-xs">
							<span class="text-ink-subtle"
								>{{ editDraft.length }}/{{ pinLimits.max_chars }}</span
							>
							<div class="flex items-center gap-1">
								<button
									class="rounded-pill px-2 py-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
									:disabled="pinSaving"
									@click="editingPinId = null"
								>
									Cancel
								</button>
								<button
									class="rounded-pill bg-brand px-3 py-1 text-on-accent disabled:opacity-40"
									:disabled="
										pinSaving || !canEditPin(pins, pinLimits, pin.id, editDraft)
									"
									@click="saveEditedPin"
								>
									Save
								</button>
							</div>
						</div>
						<p v-if="editHint" class="mt-1 text-xs text-ink-subtle">
							{{ editHint }}
						</p>
					</template>
					<template v-else>
						<p class="whitespace-pre-wrap break-words">{{ pin.content }}</p>
						<div class="mt-1 flex items-center justify-end gap-1 text-xs">
							<button
								class="rounded-pill px-2 py-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
								:disabled="pinSaving"
								@click="beginEditPin(pin)"
							>
								Edit
							</button>
							<button
								class="rounded-pill px-2 py-1 text-ink-subtle hover:bg-surface-hover hover:text-red-500"
								:disabled="pinSaving"
								@click="deletePin(pin)"
							>
								Remove
							</button>
						</div>
					</template>
				</li>
			</ul>
			<div>
				<textarea
					ref="pinInput"
					v-model="pinDraft"
					rows="2"
					:maxlength="pinLimits.max_chars"
					:disabled="pinSaving"
					placeholder="Add a fact…"
					aria-label="New pinned fact"
					class="w-full resize-none rounded-lg border border-line bg-surface-sunken px-2 py-1.5 text-sm text-ink outline-hidden placeholder:text-ink-subtle focus:border-line-focus disabled:opacity-60"
				></textarea>
				<div class="mt-1 flex items-center justify-between gap-2 text-xs">
					<span class="text-ink-subtle"
						>{{ pinDraft.length }}/{{ pinLimits.max_chars }}</span
					>
					<button
						class="rounded-pill bg-brand px-3 py-1 text-on-accent disabled:opacity-40"
						:disabled="pinSaving || !canSubmitPin"
						@click="submitPin"
					>
						Add
					</button>
				</div>
				<p v-if="pinHint" class="mt-1 text-xs text-ink-subtle">{{ pinHint }}</p>
			</div>
			<p v-if="pinError" class="text-xs text-red-500">{{ pinError }}</p>
		</section>
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
			@scroll.passive="quoteButton = null"
		>
			<p
				v-if="!state.messages.length && !loading"
				class="text-sm text-ink-subtle"
			>
				Ask about this workspace: what is overdue, how much time went into a
				category this week, what a task is about.
			</p>
			<template v-for="m in state.messages" :key="m.id">
				<div
					v-if="separators.get(m.id)"
					class="flex items-center gap-2 text-xs text-ink-subtle"
				>
					<span class="h-px flex-1 bg-line"></span>
					<span>{{ separators.get(m.id) }}</span>
					<span class="h-px flex-1 bg-line"></span>
				</div>
				<AgentMessage
					:message="m"
					:decide="decide"
					:is-quote-available="isQuoteAvailable"
					@pin="pinMessage"
					@reply="replyTo"
					@jump="jumpToQuote"
				/>
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
			<ul
				v-if="quoteChips.length"
				class="mb-2 flex flex-wrap gap-1.5"
				aria-label="Quoted messages"
			>
				<li
					v-for="(chip, i) in quoteChips"
					:key="`${quotes[i].message_id}-${i}`"
					class="flex max-w-full items-center gap-1 rounded-pill border border-line bg-surface-sunken py-0.5 pl-2.5 pr-1 text-xs text-ink-subtle"
				>
					<Quote class="h-3 w-3 shrink-0" />
					<span class="truncate">
						<span class="font-medium text-ink">{{ chip.author }}</span>
						<span v-if="chip.time"> · {{ chip.time }}</span>
						: {{ chip.text }}
					</span>
					<button
						class="shrink-0 rounded-pill p-0.5 hover:bg-surface-hover hover:text-ink"
						aria-label="Remove quote"
						@click="removeQuoteDraft(i)"
					>
						<X class="h-3 w-3" />
					</button>
				</li>
			</ul>
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
					class="min-w-0 flex-1 bg-transparent text-sm text-ink outline-hidden placeholder:text-ink-subtle"
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
					class="flex h-7 w-7 items-center justify-center rounded-pill bg-brand text-on-accent disabled:opacity-40"
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
		<button
			v-if="quoteButton && view === 'chat'"
			data-quote-button
			class="fixed z-50 flex -translate-x-1/2 items-center gap-1 rounded-pill border border-line bg-surface px-2.5 py-1 text-xs text-ink shadow-lg hover:bg-surface-hover"
			:style="{ top: `${quoteButton.top}px`, left: `${quoteButton.left}px` }"
			@mousedown.prevent
			@click="quoteSelection"
		>
			<Quote class="h-3 w-3" /> Quote
		</button>
	</aside>
</template>

<style>
	::highlight(agent-quote) {
		background-color: rgba(250, 204, 21, 0.45);
		color: inherit;
	}
	.dark ::highlight(agent-quote) {
		background-color: rgba(250, 204, 21, 0.3);
	}
	.agent-msg-flash {
		animation: agent-msg-flash 2s ease-out;
	}
	@keyframes agent-msg-flash {
		0%,
		60% {
			background-color: rgba(250, 204, 21, 0.3);
		}
		100% {
			background-color: transparent;
		}
	}
</style>
