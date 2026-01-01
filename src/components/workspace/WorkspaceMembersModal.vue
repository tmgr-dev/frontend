<template>
	<Dialog :open="modelValue" @update:open="$emit('update:modelValue', $event)">
		<DialogContent class="max-w-2xl max-h-[80vh] overflow-y-auto">
			<DialogHeader>
				<DialogTitle>Workspace Members</DialogTitle>
				<DialogDescription>
					Manage members of this workspace. Only the workspace owner can remove members.
				</DialogDescription>
			</DialogHeader>

			<div v-if="loading" class="flex items-center justify-center py-8">
				<Loader />
			</div>

			<div v-else-if="error" class="text-center py-8">
				<p class="text-red-500 mb-4">{{ error }}</p>
				<Button @click="loadMembers" variant="outline">Retry</Button>
			</div>

			<div v-else class="space-y-3">
				<div
					v-for="member in members"
					:key="member.id"
					class="flex items-center justify-between p-3 border rounded-lg dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors"
				>
					<div class="flex items-center gap-3 flex-1">
						<Avatar class="h-10 w-10">
							<AvatarImage v-if="member.avatar" :src="member.avatar" :alt="member.name" />
							<AvatarFallback>
								{{ getInitials(member.name) }}
							</AvatarFallback>
						</Avatar>
						
						<div class="flex-1">
							<div class="flex items-center gap-2">
								<p class="font-medium">{{ member.name }}</p>
								<span
									v-if="member.id === ownerId"
									class="px-2 py-0.5 text-xs font-medium rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300"
								>
									Owner
								</span>
							</div>
							<p v-if="member.email" class="text-sm text-gray-600 dark:text-gray-400">
								{{ member.email }}
							</p>
						</div>
					</div>

					<Button
						v-if="canRemoveMember(member)"
						@click="handleRemoveMember(member)"
						variant="destructive"
						size="sm"
						:disabled="removing"
					>
						<Loader v-if="removingMemberId === member.id" is-mini class="mr-2" />
						Remove
					</Button>
				</div>

				<div v-if="members.length === 0" class="text-center py-8 text-gray-500">
					<p>No members found</p>
				</div>
			</div>

			<DialogFooter class="mt-6">
				<Button @click="openWorkspaceSettings" variant="outline">
					Workspace Settings
				</Button>
				<Button @click="$emit('update:modelValue', false)" variant="default">
					Close
				</Button>
			</DialogFooter>
		</DialogContent>
	</Dialog>

	<AlertDialog v-model:open="showConfirmDialog">
		<AlertDialogContent>
			<AlertDialogHeader>
				<AlertDialogTitle>Remove Member</AlertDialogTitle>
				<AlertDialogDescription>
					Are you sure you want to remove <strong>{{ memberToRemove?.name }}</strong> from this workspace?
					This action cannot be undone.
				</AlertDialogDescription>
			</AlertDialogHeader>
			<AlertDialogFooter>
				<AlertDialogCancel @click="showConfirmDialog = false">Cancel</AlertDialogCancel>
				<AlertDialogAction
					@click="confirmRemove"
					class="bg-red-600 hover:bg-red-700 text-white"
				>
					Remove
				</AlertDialogAction>
			</AlertDialogFooter>
		</AlertDialogContent>
	</AlertDialog>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useStore } from 'vuex';
import { getWorkspaceMembers, removeMemberFromWorkspace, type WorkspaceMember } from '@/actions/tmgr/workspaces';
import { useToast } from '@/components/ui/toast/use-toast';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '@/components/ui/dialog';
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
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import Loader from '@/components/loaders/Loader.vue';

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
	'memberRemoved': [memberId: number];
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

const getInitials = (name: string) => {
	return name
		.split(' ')
		.map(part => part.charAt(0))
		.join('')
		.toUpperCase()
		.slice(0, 2);
};

const loadMembers = async () => {
	if (!props.workspaceId) return;
	
	loading.value = true;
	error.value = null;
	
	try {
		members.value = await getWorkspaceMembers(props.workspaceId);
	} catch (e: any) {
		console.error('Error loading workspace members:', e);
		error.value = e.response?.data?.message || 'Failed to load workspace members';
		toast({
			title: 'Error',
			description: error.value,
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
		await removeMemberFromWorkspace(props.workspaceId, memberToRemove.value.id);
		
		toast({
			title: 'Member removed',
			description: `${memberToRemove.value.name} has been removed from the workspace`,
		});
		
		emit('memberRemoved', memberToRemove.value.id);
		
		await loadMembers();
	} catch (e: any) {
		console.error('Error removing member:', e);
		const errorMessage = e.response?.data?.message || 'Failed to remove member';
		toast({
			title: 'Error',
			description: errorMessage,
			variant: 'destructive',
		});
	} finally {
		removing.value = false;
		removingMemberId.value = null;
		memberToRemove.value = null;
	}
};

const openWorkspaceSettings = () => {
	emit('update:modelValue', false);
	const currentWorkspace = store.state.workspaces?.find((w: any) => w.id === props.workspaceId);
	if (currentWorkspace) {
		router.push(`/settings/workspaces`);
	}
};

watch(() => props.modelValue, (newValue) => {
	if (newValue) {
		loadMembers();
	}
}, { immediate: true });
</script>

<style scoped>
</style>

