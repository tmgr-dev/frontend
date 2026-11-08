<template>
	<Dialog :open="modelValue" @update:open="$emit('update:modelValue', $event)">
		<DialogContent class="max-h-[80vh] max-w-2xl overflow-y-auto">
			<DialogHeader>
				<DialogTitle>{{
					$t('workspace.workspaceMembersModal.title')
				}}</DialogTitle>
				<DialogDescription>
					{{ $t('workspace.workspaceMembersModal.description') }}
				</DialogDescription>
			</DialogHeader>

			<div v-if="loading" class="flex items-center justify-center py-8">
				<Loader />
			</div>

			<div v-else-if="error" class="py-8 text-center">
				<p class="mb-4 text-destructive">{{ error }}</p>
				<Button @click="loadMembers" variant="outline">{{
					$t('workspace.workspaceMembersModal.retry')
				}}</Button>
			</div>

			<div v-else class="space-y-3">
				<div
					v-for="member in members"
					:key="member.id"
					class="flex cursor-pointer items-center justify-between rounded-lg border border-border transition-colors hover:bg-muted/50"
					@click="openMember(member)"
				>
					<button
						type="button"
						:aria-label="
							$t('workspace.workspaceMembersModal.openMember', {
								name: member.name,
							})
						"
						class="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-lg p-3 text-left outline-hidden"
						@click.stop="openMember(member)"
					>
						<UserAvatar
							:user-id="member.id"
							:name="member.name"
							:has-avatar="member.has_avatar ?? false"
							:size="40"
						/>

						<span class="block min-w-0 flex-1">
							<span class="flex items-center gap-2">
								<span class="font-medium text-ink">{{ member.name }}</span>
								<span
									v-if="member.id === ownerId"
									class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold text-muted-foreground uppercase"
								>
									{{ $t('workspace.workspaceMembersModal.owner') }}
								</span>
							</span>
							<span v-if="member.email" class="block text-sm text-ink-subtle">
								{{ member.email }}
							</span>
						</span>
					</button>

					<div v-if="canRemoveMember(member)" class="mr-3" @click.stop>
						<Button
							@click="handleRemoveMember(member)"
							variant="destructive"
							size="sm"
							:disabled="removing"
						>
							<Loader
								v-if="removingMemberId === member.id"
								is-mini
								class="mr-2"
							/>
							{{ $t('workspace.workspaceMembersModal.remove') }}
						</Button>
					</div>
				</div>

				<div
					v-if="members.length === 0"
					class="py-8 text-center text-sm text-ink-subtle"
				>
					<p>{{ $t('workspace.workspaceMembersModal.empty') }}</p>
				</div>
			</div>

			<DialogFooter class="mt-6">
				<Button @click="openWorkspaceSettings" variant="outline">
					{{ $t('workspace.workspaceMembersModal.workspaceSettings') }}
				</Button>
				<Button @click="$emit('update:modelValue', false)" variant="default">
					{{ $t('workspace.workspaceMembersModal.close') }}
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>

	<AlertDialog v-model:open="showConfirmDialog">
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>{{
					$t('workspace.workspaceMembersModal.removeTitle')
				}}</AlertDialogTitle>
				<AlertDialogDescription>
					<i18n-t
						keypath="workspace.workspaceMembersModal.removeConfirm"
						tag="span"
					>
						<template #name>
							<strong>{{ memberToRemove?.name }}</strong>
						</template>
					</i18n-t>
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel @click="showConfirmDialog = false">{{
					$t('workspace.workspaceMembersModal.cancel')
				}}</AlertDialogCancel>
				<AlertDialogAction
					@click="confirmRemove"
					class="bg-destructive text-destructive-foreground shadow-xs hover:bg-destructive/90"
				>
					{{ $t('workspace.workspaceMembersModal.remove') }}
				</AlertDialogAction>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>

<script setup lang="ts">
	import { t } from '@/i18n';
	import {
		getWorkspaceMembers,
		removeMemberFromWorkspace,
		type WorkspaceMember,
	} from '@/actions/tmgr/workspaces';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import Loader from '@/components/loaders/Loader.vue';
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
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { useToast } from '@/components/ui/toast/use-toast';
	import { computed, ref, watch } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	interface ExtendedWorkspaceMember extends WorkspaceMember {
		email?: string;
		avatar?: string;
	}

	interface Props {
		modelValue: boolean;
		workspaceId: number;
	}

	const props = defineProps<Props>();
	const emit = defineEmits<{
		'update:modelValue': [value: boolean];
		memberRemoved: [memberId: number];
	}>();

	const store = useStore();
	const router = useRouter();
	const { toast } = useToast();

	const members = ref<ExtendedWorkspaceMember[]>([]);
	const loading = ref(false);
	const error = ref<string | null>(null);
	const removing = ref(false);
	const removingMemberId = ref<number | null>(null);
	const showConfirmDialog = ref(false);
	const memberToRemove = ref<ExtendedWorkspaceMember | null>(null);

	const currentUserId = computed(() => store.state.user?.id);
	const ownerId = computed(() => {
		const workspaces = store.state.workspaces || [];
		const workspace = workspaces.find((w: any) => w.id === props.workspaceId);
		return workspace?.user_id;
	});

	const canRemoveMember = (member: ExtendedWorkspaceMember) => {
		return currentUserId.value === ownerId.value && member.id !== ownerId.value;
	};

	const loadMembers = async () => {
		if (!props.workspaceId) return;

		loading.value = true;
		error.value = null;

		try {
			members.value = await getWorkspaceMembers(props.workspaceId);
		} catch (e: any) {
			console.error('Error loading workspace members:', e);
			error.value =
				e.response?.data?.message ||
				t('workspace.workspaceMembersModal.loadFailed');
			toast({
				title: t('workspace.workspaceMembersModal.errorTitle'),
				description: error.value ?? undefined,
				variant: 'destructive',
			});
		} finally {
			loading.value = false;
		}
	};

	const handleRemoveMember = (member: ExtendedWorkspaceMember) => {
		memberToRemove.value = member;
		showConfirmDialog.value = true;
	};

	const confirmRemove = async () => {
		if (!memberToRemove.value) return;

		removing.value = true;
		removingMemberId.value = memberToRemove.value.id;
		showConfirmDialog.value = false;

		try {
			await removeMemberFromWorkspace(
				props.workspaceId,
				memberToRemove.value.id,
			);

			toast({
				title: t('workspace.workspaceMembersModal.removed'),
				description: t('workspace.workspaceMembersModal.removedDescription', {
					name: memberToRemove.value.name,
				}),
			});

			emit('memberRemoved', memberToRemove.value.id);

			await loadMembers();
		} catch (e: any) {
			console.error('Error removing member:', e);
			const errorMessage =
				e.response?.data?.message ||
				t('workspace.workspaceMembersModal.removeFailed');
			toast({
				title: t('workspace.workspaceMembersModal.errorTitle'),
				description: errorMessage,
				variant: 'destructive',
			});
		} finally {
			removing.value = false;
			removingMemberId.value = null;
			memberToRemove.value = null;
		}
	};

	const openMember = (member: ExtendedWorkspaceMember) => {
		const workspaceCode =
			store.getters.workspaceById(props.workspaceId)?.code ||
			store.getters.currentWorkspace?.code;
		if (!workspaceCode) return;
		emit('update:modelValue', false);
		router.push({
			name: 'WorkspaceMember',
			params: { workspace_code: workspaceCode, user_id: member.id },
		});
	};

	const openWorkspaceSettings = () => {
		emit('update:modelValue', false);
		const currentWorkspace = store.state.workspaces?.find(
			(w: any) => w.id === props.workspaceId,
		);
		if (currentWorkspace) {
			router.push(`/settings/workspaces`);
		}
	};

	watch(
		() => props.modelValue,
		(newValue) => {
			if (newValue) {
				loadMembers();
			}
		},
		{ immediate: true },
	);
</script>

<style scoped></style>
