<template>
	<Dialog :open="open" @update:open="(v) => $emit('update:open', v)">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('chats.newMessage') }}</DialogTitle>
			</DialogHeader>
			<MemberPicker v-model="memberIds" :members="members" />
			<p v-if="failed" class="text-sm text-destructive">
				{{ $t('chats.createFailed') }}
			</p>
			<DialogFooter>
				<Button variant="outline" @click="$emit('update:open', false)">{{
					$t('common.actions.cancel')
				}}</Button>
				<Button :disabled="busy || !memberIds.length" @click="submit">{{
					$t('chats.startConversation')
				}}</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { openDm } from '@/actions/tmgr/chats';
	import MemberPicker from '@/components/chats/MemberPicker.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { defineComponent, ref, watch, type PropType } from 'vue';

	export default defineComponent({
		name: 'NewMessageDialog',
		components: {
			Button,
			Dialog,
			DialogContent,
			DialogFooter,
			DialogHeader,
			DialogTitle,
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
		emits: ['update:open', 'opened'],
		setup(props, { emit }) {
			const memberIds = ref<number[]>([]);
			const busy = ref(false);
			const failed = ref(false);

			watch(
				() => props.open,
				(open) => {
					if (!open) return;
					memberIds.value = [];
					failed.value = false;
				},
			);

			const submit = async () => {
				busy.value = true;
				failed.value = false;
				try {
					emit('opened', await openDm(props.workspaceId, memberIds.value));
					emit('update:open', false);
				} catch {
					failed.value = true;
				} finally {
					busy.value = false;
				}
			};

			return { memberIds, busy, failed, submit };
		},
	});
</script>
