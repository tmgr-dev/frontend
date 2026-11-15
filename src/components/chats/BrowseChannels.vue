<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.browseChannels') }}</DialogTitle>
			</DialogHeader>
			<p v-if="loading" class="text-sm text-ink-subtle">
				{{ $t('common.states.loading') }}
			</p>
			<p
				v-else-if="!channels.length && !closed.length"
				class="text-sm text-ink-subtle"
			>
				{{ $t('chats.noChannelsToJoin') }}
			</p>
			<ul
				v-else-if="channels.length"
				class="divide-y divide-line max-h-80 overflow-y-auto rounded-card border border-line bg-surface"
			>
				<li
					v-for="channel in channels"
					:key="channel.id"
					class="flex items-center gap-3 px-3 py-2"
				>
					<div class="min-w-0 flex-1">
						<p class="truncate text-sm font-medium text-ink">
							# {{ channel.title }}
						</p>
						<p class="truncate text-xs text-ink-subtle">
							{{
								channel.topic || $t('chats.memberCount', channel.member_count)
							}}
						</p>
					</div>
					<Button
						size="sm"
						:disabled="joining === channel.id"
						@click="join(channel)"
						>{{ $t('chats.join') }}</Button
					>
				</li>
			</ul>
			<template v-if="!loading && closed.length">
				<h3 class="text-sm font-semibold text-ink">
					{{ $t('chats.privateNotIn') }}
				</h3>
				<ul
					class="divide-y divide-line max-h-48 overflow-y-auto rounded-card border border-line bg-surface"
					data-testid="chat-closed-channels"
				>
					<li
						v-for="channel in closed"
						:key="channel.id"
						class="flex items-center gap-3 px-3 py-2"
					>
						<div class="min-w-0 flex-1">
							<p class="truncate text-sm font-medium text-ink">
								{{ channel.title }}
							</p>
							<p class="truncate text-xs text-ink-subtle">
								{{ $t('chats.memberCount', channel.member_count) }}
							</p>
						</div>
						<Button
							v-if="channel.can_delete"
							size="sm"
							variant="destructive"
							@click="pendingDelete = channel"
							>{{ $t('chats.deleteChat') }}</Button
						>
					</li>
				</ul>
			</template>
			<AlertDialog
				:open="!!pendingDelete"
				@update:open="(v) => !v && (pendingDelete = null)"
			>
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle>{{ $t('chats.deleteTitle') }}</AlertDialogTitle>
						<AlertDialogDescription>{{
							$t('chats.deleteBody')
						}}</AlertDialogDescription>
					</AlertDialogHeader>
					<AlertDialogFooter>
						<AlertDialogCancel>{{
							$t('common.actions.cancel')
						}}</AlertDialogCancel>
						<AlertDialogAction
							class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
							@click="removeClosed"
							>{{ $t('common.actions.delete') }}</AlertDialogAction
						>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { deleteChat, joinChat, listChats } from '@/actions/tmgr/chats';
	import type { Chat } from '@/actions/tmgr/chats';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { isClosedChannel, useChats } from '@/composable/useChats';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'BrowseChannels',
		components: {
			AlertDialog,
			AlertDialogAction,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			Dialog,
			DialogContent,
			DialogHeader,
			DialogTitle,
		},
		props: {
			open: { type: Boolean, required: true },
			workspaceId: { type: Number, required: true },
		},
		emits: ['update:open', 'joined'],
		setup(props, { emit }) {
			const { removeChat } = useChats();
			const channels = ref<Chat[]>([]);
			const closed = ref<Chat[]>([]);
			const pendingDelete = ref<Chat | null>(null);
			const loading = ref(false);
			const joining = ref<number | null>(null);

			watch(
				() => props.open,
				async (open) => {
					if (!open) return;
					loading.value = true;
					try {
						const all = await listChats(props.workspaceId);
						closed.value = all.filter(
							(c) => isClosedChannel(c) && !c.archived_at,
						);
						channels.value = all.filter(
							(c) =>
								c.kind === 'channel' &&
								!c.joined &&
								c.visibility === 'workspace' &&
								!c.archived_at,
						);
					} finally {
						loading.value = false;
					}
				},
			);

			const join = async (channel: Chat) => {
				joining.value = channel.id;
				try {
					emit('joined', await joinChat(channel.id));
					emit('update:open', false);
				} finally {
					joining.value = null;
				}
			};

			const removeClosed = async () => {
				const channel = pendingDelete.value;
				if (!channel) return;
				await deleteChat(channel.id);
				removeChat(channel.id);
				closed.value = closed.value.filter((c) => c.id !== channel.id);
				pendingDelete.value = null;
			};

			return {
				channels,
				closed,
				loading,
				joining,
				pendingDelete,
				join,
				removeClosed,
			};
		},
	});
</script>
