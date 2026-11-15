<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.newChannel') }}</DialogTitle>
			</DialogHeader>
			<form class="space-y-3" @submit.prevent="submit">
				<Input
					v-model="title"
					:placeholder="$t('chats.channelName')"
					data-testid="chat-channel-title"
				/>
				<Input v-model="topic" :placeholder="$t('chats.channelTopic')" />
				<div class="space-y-1 text-sm text-ink">
					<label class="flex items-center gap-2">
						<input v-model="visibility" type="radio" value="workspace" />
						{{ $t('chats.publicChannel') }}
					</label>
					<label class="flex items-center gap-2">
						<input v-model="visibility" type="radio" value="members" />
						{{ $t('chats.privateChannel') }}
					</label>
				</div>
				<MemberPicker v-model="memberIds" :members="members" />
				<p v-if="failed" class="text-sm text-destructive">
					{{ $t('chats.createFailed') }}
				</p>
				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						@click="$emit('update:open', false)"
						>{{ $t('common.actions.cancel') }}</Button
					>
					<Button type="submit" :disabled="busy || !title.trim()">{{
						$t('common.actions.create')
					}}</Button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { createChannel } from '@/actions/tmgr/chats';
	import type { Chat, ChatVisibility } from '@/actions/tmgr/chats';
	import MemberPicker from '@/components/chats/MemberPicker.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import { defineComponent, ref, watch, type PropType } from 'vue';

	export default defineComponent({
		name: 'NewChannelDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
			Input,
			MemberPicker,
		},
		props: {
			open: { type: Boolean, required: true },
			workspaceId: { type: Number, required: true },
			members: {
				type: Array as PropType<Array<{ id: number; name: string }>>,
				required: true,
			},
		},
		emits: ['update:open', 'created'],
		setup(props, { emit }) {
			const title = ref('');
			const topic = ref('');
			const visibility = ref<ChatVisibility>('workspace');
			const memberIds = ref<number[]>([]);
			const busy = ref(false);
			const failed = ref(false);

			watch(
				() => props.open,
				(open) => {
					if (!open) return;
					title.value = '';
					topic.value = '';
					visibility.value = 'workspace';
					memberIds.value = [];
					failed.value = false;
				},
			);

			const submit = async () => {
				busy.value = true;
				failed.value = false;
				try {
					const chat: Chat = await createChannel(props.workspaceId, {
						title: title.value.trim(),
						topic: topic.value.trim() || null,
						visibility: visibility.value,
						member_ids: memberIds.value,
					});
					emit('created', chat);
					emit('update:open', false);
				} catch {
					failed.value = true;
				} finally {
					busy.value = false;
				}
			};

			return { title, topic, visibility, memberIds, busy, failed, submit };
		},
	});
</script>
