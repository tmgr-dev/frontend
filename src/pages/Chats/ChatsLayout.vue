<template>
	<div class="flex h-[calc(100vh-4rem)] min-h-0" data-testid="chats-layout">
		<aside
			class="w-full shrink-0 flex-col gap-3 overflow-y-auto border-r border-line p-3 md:flex md:w-72"
			:class="activeId ? 'hidden' : 'flex'"
		>
			<h1 class="px-2 text-lg font-bold text-ink">{{ $t('chats.title') }}</h1>
			<ChatSearch
				v-if="workspace"
				:workspace-id="workspace.id"
				@open="openChatById"
			/>
			<div class="flex flex-wrap gap-2">
				<Button
					size="sm"
					data-testid="chat-new-channel"
					@click="showNewChannel = true"
				>
					<Plus />
					{{ $t('chats.newChannel') }}
				</Button>
				<Button
					size="sm"
					variant="outline"
					data-testid="chat-new-message"
					@click="showNewMessage = true"
				>
					{{ $t('chats.newMessage') }}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					data-testid="chat-browse"
					@click="showBrowse = true"
				>
					{{ $t('chats.browseChannels') }}
				</Button>
			</div>
			<p v-if="!chatsLoaded" class="px-2 text-sm text-ink-subtle">
				{{ $t('common.states.loading') }}
			</p>
			<ChatList
				v-else
				:chats="chats"
				:active-id="activeId"
				:workspace-code="workspace?.code ?? ''"
				:self-id="selfId"
			/>
		</aside>
		<main class="min-w-0 flex-1" :class="activeId ? 'flex' : 'hidden md:flex'">
			<router-view v-if="activeId" />
			<div v-else class="flex flex-1 items-center justify-center p-6">
				<p class="text-sm text-ink-subtle">{{ $t('chats.selectChat') }}</p>
			</div>
		</main>

		<NewChannelDialog
			v-if="workspace"
			v-model:open="showNewChannel"
			:workspace-id="workspace.id"
			:members="members"
			@created="openCreated"
		/>
		<NewMessageDialog
			v-if="workspace"
			v-model:open="showNewMessage"
			:workspace-id="workspace.id"
			:members="members"
			@opened="openCreated"
		/>
		<BrowseChannels
			v-if="workspace"
			v-model:open="showBrowse"
			:workspace-id="workspace.id"
			@joined="openCreated"
		/>
	</div>
</template>

<script lang="ts">
	import type { Chat } from '@/actions/tmgr/chats';
	import { getWorkspaceMembers } from '@/actions/tmgr/workspaces';
	import BrowseChannels from '@/components/chats/BrowseChannels.vue';
	import ChatList from '@/components/chats/ChatList.vue';
	import ChatSearch from '@/components/chats/ChatSearch.vue';
	import NewChannelDialog from '@/components/chats/NewChannelDialog.vue';
	import NewMessageDialog from '@/components/chats/NewMessageDialog.vue';
	import { Button } from '@/components/ui/button';
	import { useChats } from '@/composable/useChats';
	import { Plus } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'ChatsLayout',
		components: {
			BrowseChannels,
			Button,
			ChatList,
			ChatSearch,
			NewChannelDialog,
			NewMessageDialog,
			Plus,
		},
		setup() {
			const route = useRoute();
			const router = useRouter();
			const store = useStore();
			const { chats, chatsLoaded, loadChats, upsertChat, start } = useChats();

			const showNewChannel = ref(false);
			const showNewMessage = ref(false);
			const showBrowse = ref(false);
			const members = ref<Array<{ id: number; name: string }>>([]);
			let stop: (() => void) | null = null;
			let initRun = 0;

			const workspace = computed(() => store.getters.currentWorkspace);
			const selfId = computed<number | null>(
				() => store.state.user?.id ?? null,
			);
			const activeId = computed(() =>
				route.params.chat_id ? Number(route.params.chat_id) : null,
			);

			const openChatById = (chatId: number) =>
				router.push(`/${workspace.value.code}/chats/${chatId}`);

			const openCreated = (chat: Chat) => {
				upsertChat(chat);
				openChatById(chat.id);
			};

			const init = async () => {
				stop?.();
				stop = null;
				const run = ++initRun;
				if (!workspace.value) return;
				chatsLoaded.value = false;
				await loadChats(workspace.value.id).catch(() => {
					chatsLoaded.value = true;
				});
				if (run !== initRun) return;
				if (selfId.value) stop = start(selfId.value, workspace.value.id);
				getWorkspaceMembers(workspace.value.id)
					.then((all) => {
						members.value = all
							.filter((m) => m.id !== selfId.value)
							.map((m) => ({ id: m.id, name: m.name }));
					})
					.catch(() => undefined);
			};

			onMounted(init);
			watch(() => workspace.value?.id, init);
			onBeforeUnmount(() => {
				initRun++;
				stop?.();
			});

			return {
				chats,
				chatsLoaded,
				workspace,
				selfId,
				activeId,
				members,
				showNewChannel,
				showNewMessage,
				showBrowse,
				openChatById,
				openCreated,
			};
		},
	});
</script>
