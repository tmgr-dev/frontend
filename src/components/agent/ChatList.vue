<template>
	<div class="flex min-h-0 flex-1 flex-col">
		<div class="shrink-0 space-y-2 border-b border-line px-4 py-3">
			<div class="flex items-center justify-between">
				<h2 class="text-sm font-semibold text-ink">Chats</h2>
				<button
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="$emit('new-chat')"
				>
					New chat
				</button>
			</div>
			<input
				v-model="text"
				type="search"
				maxlength="100"
				aria-label="Search chats"
				placeholder="Search chats…"
				class="w-full rounded-pill border border-line bg-surface-sunken px-4 py-1.5 text-sm text-ink outline-hidden placeholder:text-ink-subtle focus:border-line-focus"
			/>
			<label class="flex items-center gap-2 text-xs text-ink-subtle">
				<input
					type="checkbox"
					:checked="showArchived"
					@change="
						$emit(
							'update:showArchived',
							($event.target as HTMLInputElement).checked,
						)
					"
				/>
				Show archived
			</label>
		</div>
		<ul class="flex-1 overflow-y-auto px-2 py-2">
			<li v-if="error" class="px-2 py-2 text-xs text-red-500">{{ error }}</li>
			<li
				v-else-if="!items.length && !loading"
				class="px-2 py-2 text-sm text-ink-subtle"
			>
				{{ effectiveQuery ? 'No chats found' : 'No chats yet' }}
			</li>
			<li
				v-for="item in items"
				:key="item.id"
				class="group flex items-center gap-1 rounded-md px-2 py-1.5"
				:class="
					item.id === currentId
						? 'bg-surface-hover'
						: 'hover:bg-surface-hover/60'
				"
			>
				<input
					v-if="renamingId === item.id"
					v-model="renameDraft"
					class="min-w-0 flex-1 rounded border border-line bg-surface-sunken px-2 py-1 text-sm text-ink outline-hidden focus:border-line-focus"
					aria-label="Chat title"
					maxlength="255"
					@keydown.enter.prevent="saveRename(item)"
					@keydown.esc.stop.prevent="renamingId = null"
					@blur="renamingId = null"
				/>
				<button
					v-else
					class="min-w-0 flex-1 text-left"
					@click="$emit('open', item.id)"
				>
					<span class="block truncate text-sm text-ink">
						{{ chatTitle(item.title) }}
					</span>
					<span class="block text-xs text-ink-subtle">
						{{ formatChatTime(item.last_message_at ?? item.created_at) }}
					</span>
				</button>
				<template v-if="confirmingId === item.id">
					<span class="shrink-0 text-xs text-ink-subtle">Delete?</span>
					<button
						class="shrink-0 rounded-pill px-2 py-0.5 text-xs text-red-500 hover:bg-surface-hover disabled:opacity-40"
						:disabled="acting"
						@click="remove(item)"
					>
						Yes
					</button>
					<button
						class="shrink-0 rounded-pill px-2 py-0.5 text-xs text-ink-subtle hover:bg-surface-hover"
						@click="confirmingId = null"
					>
						No
					</button>
				</template>
				<div
					v-else-if="renamingId !== item.id"
					class="flex shrink-0 items-center opacity-60 group-hover:opacity-100"
				>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
						aria-label="Rename chat"
						@click="startRename(item)"
					>
						<Pencil class="h-3.5 w-3.5" />
					</button>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
						:aria-label="item.archived ? 'Unarchive chat' : 'Archive chat'"
						:disabled="acting"
						@click="toggleArchive(item)"
					>
						<ArchiveRestore v-if="item.archived" class="h-3.5 w-3.5" />
						<Archive v-else class="h-3.5 w-3.5" />
					</button>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-red-500"
						aria-label="Delete chat"
						@click="confirmingId = item.id"
					>
						<Trash2 class="h-3.5 w-3.5" />
					</button>
				</div>
			</li>
		</ul>
	</div>
</template>

<script lang="ts">
	import {
		archiveConversation,
		deleteConversation,
		listConversations,
		renameConversation,
	} from '@/actions/tmgr/agent';
	import type { AgentConversationItem } from '@/types/agent';
	import {
		chatTitle,
		formatChatTime,
		nextChatAfterRemoval,
	} from '@/utils/agentChat';
	import { createRequestSequence } from '@/utils/requestSequence';
	import { Archive, ArchiveRestore, Pencil, Trash2 } from 'lucide-vue-next';
	import type { PropType } from 'vue';
	import { defineComponent, nextTick, onUnmounted, ref, watch } from 'vue';

	const MIN_QUERY = 2;
	const DEBOUNCE_MS = 300;

	export default defineComponent({
		name: 'ChatList',
		components: { Archive, ArchiveRestore, Pencil, Trash2 },
		props: {
			workspaceId: { type: Number, required: true },
			currentId: { type: Number as PropType<number | null>, default: null },
			query: { type: String, default: '' },
			showArchived: { type: Boolean, default: false },
			refreshKey: { type: Number, default: 0 },
		},
		emits: [
			'open',
			'new-chat',
			'removed',
			'update:query',
			'update:showArchived',
		],
		setup(props, { emit }) {
			const items = ref<AgentConversationItem[]>([]);
			const loading = ref(false);
			const error = ref<string | null>(null);
			const text = ref(props.query);
			const effectiveQuery = ref('');
			const renamingId = ref<number | null>(null);
			const renameDraft = ref('');
			const confirmingId = ref<number | null>(null);
			const acting = ref(false);
			const requests = createRequestSequence();
			let debounce: ReturnType<typeof setTimeout> | null = null;

			const toEffective = (value: string): string => {
				const trimmed = value.trim();
				return trimmed.length >= MIN_QUERY ? trimmed : '';
			};
			effectiveQuery.value = toEffective(text.value);

			const refresh = async (): Promise<void> => {
				const token = requests.begin();
				loading.value = true;
				error.value = null;
				try {
					const result = await listConversations(props.workspaceId, {
						q: effectiveQuery.value,
						archived: props.showArchived,
					});
					if (requests.isCurrent(token)) items.value = result;
				} catch {
					if (requests.isCurrent(token)) error.value = 'Could not load chats.';
				} finally {
					if (requests.isCurrent(token)) loading.value = false;
				}
			};

			watch(text, (value) => {
				emit('update:query', value);
				if (debounce !== null) clearTimeout(debounce);
				debounce = setTimeout(() => {
					const next = toEffective(value);
					if (next !== effectiveQuery.value) effectiveQuery.value = next;
				}, DEBOUNCE_MS);
			});

			watch(
				() =>
					[
						props.workspaceId,
						effectiveQuery.value,
						props.showArchived,
						props.refreshKey,
					] as const,
				refresh,
				{ immediate: true },
			);

			const startRename = (item: AgentConversationItem): void => {
				confirmingId.value = null;
				error.value = null;
				renameDraft.value = item.title ?? '';
				renamingId.value = item.id;
				nextTick(() => {
					const field = document.querySelector<HTMLInputElement>(
						'input[aria-label="Chat title"]',
					);
					field?.focus();
					field?.select();
				});
			};

			const saveRename = async (item: AgentConversationItem): Promise<void> => {
				const title = renameDraft.value.trim();
				renamingId.value = null;
				if (title === (item.title ?? '')) return;
				error.value = null;
				try {
					const updated = await renameConversation(item.id, title);
					items.value = items.value.map((i) =>
						i.id === item.id ? { ...i, title: updated.title ?? null } : i,
					);
				} catch {
					error.value = 'Could not rename the chat.';
				}
			};

			const dropItem = (
				id: number,
				action: 'archive' | 'unarchive' | 'delete',
			): void => {
				const nextId = props.showArchived
					? null
					: nextChatAfterRemoval(items.value, id);
				requests.begin();
				loading.value = false;
				items.value = items.value.filter((i) => i.id !== id);
				emit('removed', { id, action, nextId });
			};

			const toggleArchive = async (
				item: AgentConversationItem,
			): Promise<void> => {
				if (acting.value) return;
				acting.value = true;
				error.value = null;
				try {
					await archiveConversation(item.id, !item.archived);
					dropItem(item.id, item.archived ? 'unarchive' : 'archive');
				} catch {
					error.value = 'Could not update the chat.';
				} finally {
					acting.value = false;
				}
			};

			const remove = async (item: AgentConversationItem): Promise<void> => {
				if (acting.value) return;
				acting.value = true;
				confirmingId.value = null;
				error.value = null;
				try {
					await deleteConversation(item.id);
					dropItem(item.id, 'delete');
				} catch {
					error.value = 'Could not delete the chat.';
				} finally {
					acting.value = false;
				}
			};

			const cancelRename = (): boolean => {
				if (renamingId.value === null) return false;
				renamingId.value = null;
				return true;
			};

			onUnmounted(() => {
				requests.dispose();
				if (debounce !== null) clearTimeout(debounce);
			});

			return {
				items,
				loading,
				error,
				text,
				effectiveQuery,
				renamingId,
				renameDraft,
				confirmingId,
				acting,
				cancelRename,
				startRename,
				saveRename,
				toggleArchive,
				remove,
				chatTitle,
				formatChatTime,
			};
		},
	});
</script>
