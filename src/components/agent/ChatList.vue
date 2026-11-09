<template>
	<div class="flex min-h-0 flex-1 flex-col">
		<div class="space-y-2 shrink-0 border-b border-line px-4 py-3">
			<div class="flex items-center justify-between">
				<h2 class="text-sm font-semibold text-ink">
					{{ $t('agents.chatList.title') }}
				</h2>
				<button
					class="rounded-pill px-2 py-1 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink"
					@click="$emit('new-chat')"
				>
					{{ $t('agents.chatList.newChat') }}
				</button>
			</div>
			<input
				v-model="text"
				type="search"
				maxlength="100"
				:aria-label="$t('agents.chatList.search')"
				:placeholder="$t('agents.chatList.searchPlaceholder')"
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
				{{ $t('agents.chatList.showArchived') }}
			</label>
		</div>
		<ul class="flex-1 overflow-y-auto px-2 py-2">
			<li v-if="error" class="px-2 py-2 text-xs text-red-500">{{ error }}</li>
			<li
				v-else-if="!items.length && !loading"
				class="px-2 py-2 text-sm text-ink-subtle"
			>
				{{
					effectiveQuery
						? $t('agents.chatList.noneFound')
						: $t('agents.chatList.noneYet')
				}}
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
					:aria-label="$t('agents.chatList.chatTitle')"
					data-chat-title-input
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
					<span class="shrink-0 text-xs text-ink-subtle">{{
						$t('agents.chatList.deleteQuestion')
					}}</span>
					<button
						class="shrink-0 rounded-pill px-2 py-0.5 text-xs text-red-500 hover:bg-surface-hover disabled:opacity-40"
						:disabled="acting"
						@click="remove(item)"
					>
						{{ $t('common.states.yes') }}
					</button>
					<button
						class="shrink-0 rounded-pill px-2 py-0.5 text-xs text-ink-subtle hover:bg-surface-hover"
						@click="confirmingId = null"
					>
						{{ $t('common.states.no') }}
					</button>
				</template>
				<div
					v-else-if="renamingId !== item.id"
					class="flex shrink-0 items-center opacity-60 group-hover:opacity-100"
				>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
						:aria-label="$t('agents.chatList.rename')"
						@click="startRename(item)"
					>
						<Pencil class="h-3.5 w-3.5" />
					</button>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-ink"
						:aria-label="
							item.archived
								? $t('agents.chatList.unarchive')
								: $t('agents.chatList.archive')
						"
						:disabled="acting"
						@click="toggleArchive(item)"
					>
						<ArchiveRestore v-if="item.archived" class="h-3.5 w-3.5" />
						<Archive v-else class="h-3.5 w-3.5" />
					</button>
					<button
						class="rounded-pill p-1 text-ink-subtle hover:bg-surface-hover hover:text-red-500"
						:aria-label="$t('agents.chatList.delete')"
						@click="confirmingId = item.id"
					>
						<Trash2 class="h-3.5 w-3.5" />
					</button>
				</div>
			</li>
			<li v-if="nextCursor" class="px-2 py-2">
				<button
					class="w-full rounded-pill px-3 py-1.5 text-xs text-ink-subtle hover:bg-surface-hover hover:text-ink disabled:opacity-40"
					:disabled="loadingMore"
					@click="loadMore"
				>
					{{ $t('agents.chatList.loadMore') }}
				</button>
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
	import { t } from '@/i18n';
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
			const loadingMore = ref(false);
			const nextCursor = ref<string | null>(null);
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
				loadingMore.value = false;
				nextCursor.value = null;
				error.value = null;
				try {
					const result = await listConversations(props.workspaceId, {
						q: effectiveQuery.value,
						archived: props.showArchived,
					});
					if (requests.isCurrent(token)) {
						items.value = result.items;
						nextCursor.value = result.nextCursor;
					}
				} catch {
					if (requests.isCurrent(token))
						error.value = t('agents.chatList.loadFailed');
				} finally {
					if (requests.isCurrent(token)) loading.value = false;
				}
			};

			const loadMore = async (): Promise<void> => {
				if (!nextCursor.value || loadingMore.value || loading.value) return;
				const token = requests.begin();
				loadingMore.value = true;
				error.value = null;
				try {
					const result = await listConversations(props.workspaceId, {
						q: effectiveQuery.value,
						archived: props.showArchived,
						before: nextCursor.value,
					});
					if (requests.isCurrent(token)) {
						const known = new Set(items.value.map((i) => i.id));
						items.value = [
							...items.value,
							...result.items.filter((i) => !known.has(i.id)),
						];
						nextCursor.value = result.nextCursor;
					}
				} catch {
					if (requests.isCurrent(token))
						error.value = t('agents.chatList.loadFailed');
				} finally {
					if (requests.isCurrent(token)) loadingMore.value = false;
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
						'input[data-chat-title-input]',
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
					error.value = t('agents.chatList.renameFailed');
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
					error.value = t('agents.chatList.updateFailed');
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
					error.value = t('agents.chatList.deleteFailed');
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
				loadingMore,
				nextCursor,
				loadMore,
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
