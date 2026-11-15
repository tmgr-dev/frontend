<template>
	<div>
		<DropdownMenu>
			<DropdownMenuTrigger as-child>
				<Button
					variant="ghost"
					size="icon"
					:aria-label="$t('chats.menu')"
					data-testid="chat-menu"
				>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" class="w-56">
				<template v-if="chat.can_read">
					<DropdownMenuSub>
						<DropdownMenuSubTrigger>{{
							$t('chats.mute')
						}}</DropdownMenuSubTrigger>
						<DropdownMenuSubContent>
							<DropdownMenuItem v-if="muted" @click="unmute">{{
								$t('chats.unmute')
							}}</DropdownMenuItem>
							<DropdownMenuItem @click="muteFor(1)">{{
								$t('chats.mute1h')
							}}</DropdownMenuItem>
							<DropdownMenuItem @click="muteFor(8)">{{
								$t('chats.mute8h')
							}}</DropdownMenuItem>
							<DropdownMenuItem @click="muteFor(24)">{{
								$t('chats.mute24h')
							}}</DropdownMenuItem>
							<DropdownMenuItem @click="setLevel('none')">{{
								$t('chats.muteUntilOn')
							}}</DropdownMenuItem>
						</DropdownMenuSubContent>
					</DropdownMenuSub>
					<DropdownMenuSub>
						<DropdownMenuSubTrigger>{{
							$t('chats.notifications.title')
						}}</DropdownMenuSubTrigger>
						<DropdownMenuSubContent>
							<DropdownMenuItem
								v-for="level in levels"
								:key="level.value"
								@click="setLevel(level.value)"
							>
								<Check
									class="mr-2 h-4 w-4"
									:class="currentLevel === level.value ? '' : 'invisible'"
								/>
								{{ level.label }}
							</DropdownMenuItem>
						</DropdownMenuSubContent>
					</DropdownMenuSub>
					<DropdownMenuItem v-if="canInvitePersona" @click="inviting = true">{{
						$t('chats.persona.invite')
					}}</DropdownMenuItem>
					<DropdownMenuItem @click="$emit('mark-read')">{{
						$t('chats.markAsRead')
					}}</DropdownMenuItem>
					<DropdownMenuSeparator />
					<DropdownMenuItem
						v-if="chat.joined && chat.kind !== 'dm'"
						@click="leave"
						>{{ $t('chats.leaveChat') }}</DropdownMenuItem
					>
					<DropdownMenuItem v-if="canArchive" @click="archive">{{
						$t('chats.archiveChannel')
					}}</DropdownMenuItem>
					<DropdownMenuItem v-if="canUnarchive" @click="unarchive">{{
						$t('chats.unarchiveChannel')
					}}</DropdownMenuItem>
				</template>
				<DropdownMenuItem
					v-if="chat.can_delete"
					class="text-destructive"
					@click="confirmDelete = true"
					>{{ $t('chats.deleteChat') }}</DropdownMenuItem
				>
			</DropdownMenuContent>
		</DropdownMenu>

		<InvitePersonaDialog
			v-model:open="inviting"
			:chat-id="chat.id"
			@invited="$emit('persona-invited')"
		/>

		<AlertDialog v-model:open="confirmDelete">
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
						@click="remove"
						>{{ $t('common.actions.delete') }}</AlertDialogAction
					>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</div>
</template>

<script lang="ts">
	import {
		archiveChat,
		deleteChat,
		leaveChat,
		setChatNotifications,
		unarchiveChat,
	} from '@/actions/tmgr/chats';
	import type { Chat, NotifyLevel } from '@/actions/tmgr/chats';
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
	import InvitePersonaDialog from '@/components/chats/InvitePersonaDialog.vue';
	import { Button } from '@/components/ui/button';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { isMuted, useChats } from '@/composable/useChats';
	import { Check, MoreHorizontal } from 'lucide-vue-next';
	import { computed, defineComponent, ref, type PropType } from 'vue';
	import { useI18n } from 'vue-i18n';

	export default defineComponent({
		name: 'ChatMenu',
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
			Check,
			InvitePersonaDialog,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuSeparator,
			DropdownMenuSub,
			DropdownMenuSubContent,
			DropdownMenuSubTrigger,
			DropdownMenuTrigger,
			MoreHorizontal,
		},
		props: {
			chat: { type: Object as PropType<Chat>, required: true },
		},
		emits: ['mark-read', 'gone', 'persona-invited'],
		setup(props, { emit }) {
			const { t } = useI18n();
			const { upsertChat, removeChat } = useChats();
			const confirmDelete = ref(false);
			const inviting = ref(false);
			const canInvitePersona = computed(
				() => props.chat.can_moderate && props.chat.can_write,
			);

			const levels: Array<{ value: NotifyLevel; label: string }> = [
				{ value: 'all', label: t('chats.notifications.all') },
				{ value: 'mentions', label: t('chats.notifications.mentions') },
				{ value: 'none', label: t('chats.notifications.none') },
			];
			const currentLevel = computed(() => props.chat.notify_level ?? 'all');
			const muted = computed(() => isMuted(props.chat));
			const canArchive = computed(
				() =>
					props.chat.can_moderate &&
					props.chat.kind === 'channel' &&
					!props.chat.archived_at,
			);
			const canUnarchive = computed(
				() =>
					props.chat.can_moderate &&
					props.chat.kind === 'channel' &&
					!!props.chat.archived_at,
			);

			const setLevel = async (level: NotifyLevel) =>
				upsertChat(await setChatNotifications(props.chat.id, level, null));
			const muteFor = async (hours: number) =>
				upsertChat(
					await setChatNotifications(
						props.chat.id,
						currentLevel.value,
						new Date(Date.now() + hours * 3600_000).toISOString(),
					),
				);
			const unmute = async () =>
				upsertChat(
					await setChatNotifications(
						props.chat.id,
						currentLevel.value === 'none' ? 'all' : currentLevel.value,
						null,
					),
				);
			const leave = async () => {
				await leaveChat(props.chat.id);
				removeChat(props.chat.id);
				emit('gone');
			};
			const archive = async () => upsertChat(await archiveChat(props.chat.id));
			const unarchive = async () =>
				upsertChat(await unarchiveChat(props.chat.id));
			const remove = async () => {
				await deleteChat(props.chat.id);
				removeChat(props.chat.id);
				emit('gone');
			};

			return {
				confirmDelete,
				inviting,
				canInvitePersona,
				levels,
				currentLevel,
				muted,
				canArchive,
				canUnarchive,
				setLevel,
				muteFor,
				unmute,
				leave,
				archive,
				unarchive,
				remove,
			};
		},
	});
</script>
