<template>
	<div class="workspace-invitations-list">
		<h4 class="text-md font-semibold mb-4">Workspace Invitations</h4>

		<div v-if="loading" class="text-center py-4">
			<div class="spinner"></div>
			<p class="mt-2 text-gray-600">Loading invitations...</p>
		</div>

		<div v-else-if="error" class="text-center py-4 text-red-500">
			<p>Error: {{ error }}</p>
			<Button @click="loadInvitations" class="mt-2">Try again</Button>
		</div>

		<div v-else-if="invitations.length === 0" class="text-center py-8 text-gray-500">
			<p>No invitations</p>
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="invitation in invitations"
				:key="invitation.id"
				:class="[
					'flex items-center justify-between p-3 border rounded-lg dark:border-gray-700',
					invitation.is_accepted ? 'bg-gray-50 dark:bg-gray-800 opacity-75' : ''
				]"
			>
				<div class="flex-1">
					<div class="flex items-center gap-2">
						<p class="font-medium">
							{{ invitation.email || 'General invitation' }}
						</p>
						<span 
							v-if="invitation.is_accepted" 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300"
						>
							Accepted
						</span>
						<span 
							v-else-if="isExpired(invitation)" 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300"
						>
							Expired
						</span>
						<span 
							v-else 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300"
						>
							Pending
						</span>
					</div>
					<p class="text-sm text-gray-500 mt-1">
						Token: {{ invitation.token.substring(0, 8) }}...
					</p>
					<p class="text-xs text-gray-400">
						<span v-if="invitation.expired_at">
							Expires: {{ formatDate(invitation.expired_at) }}
						</span>
						<span v-else class="text-green-500">No expiration</span>
					</p>
				</div>

				<Button
					variant="destructive"
					size="sm"
					@click="handleRevoke(invitation.id)"
					:disabled="revoking === invitation.id || invitation.is_accepted"
				>
					{{ revoking === invitation.id ? 'Revoking...' : 'Revoke' }}
				</Button>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { ref, onMounted, computed } from 'vue';
	import { Button } from '@/components/ui/button';
	import { getWorkspaceInvitations, revokeInvitation, type WorkspaceInvitation } from '@/actions/tmgr/invitations';
	import { useToast } from '@/components/ui/toast';
	import { CircleCheckBigIcon } from 'lucide-vue-next';
	import store from '@/store';

	const toaster = useToast();
	const invitations = ref<WorkspaceInvitation[]>([]);
	const loading = ref(false);
	const error = ref<string | null>(null);
	const revoking = ref<number | null>(null);

	const currentWorkspaceId = computed(() => {
		const currentWorkspaceSetting = store.state.user?.settings?.find(
			(setting: any) => setting.key === 'current_workspace'
		);
		return currentWorkspaceSetting?.value;
	});

	const loadInvitations = async () => {
		if (!currentWorkspaceId.value) return;

		loading.value = true;
		error.value = null;

		try {
			invitations.value = await getWorkspaceInvitations(Number(currentWorkspaceId.value));
		} catch (err: any) {
			error.value = err.response?.data?.message || 'Failed to load invitations';
			console.error('Error loading invitations:', err);
		} finally {
			loading.value = false;
		}
	};

	const handleRevoke = async (invitationId: number) => {
		if (!currentWorkspaceId.value) return;
		if (!confirm('Are you sure you want to revoke this invitation?')) return;

		revoking.value = invitationId;

		try {
			await revokeInvitation(Number(currentWorkspaceId.value), invitationId);
			invitations.value = invitations.value.filter(inv => inv.id !== invitationId);
			
			toaster.toast({
				title: 'Invitation revoked',
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (err: any) {
			error.value = err.response?.data?.message || 'Failed to revoke invitation';
			toaster.toast({
				title: 'Error',
				description: error.value,
				variant: 'destructive',
			});
			console.error('Error revoking invitation:', err);
		} finally {
			revoking.value = null;
		}
	};

	const formatDate = (dateString: string | null) => {
		if (!dateString) return 'Never';
		const date = new Date(dateString);
		return date.toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		});
	};

	const isExpired = (invitation: WorkspaceInvitation) => {
		if (!invitation.expired_at) return false;
		return new Date(invitation.expired_at) < new Date();
	};

	onMounted(() => {
		loadInvitations();
	});

	defineExpose({
		loadInvitations,
	});
</script>

<style scoped>
.spinner {
	width: 24px;
	height: 24px;
	border: 3px solid rgba(0, 0, 0, 0.1);
	border-top-color: currentColor;
	border-radius: 50%;
	animation: spin 0.8s linear infinite;
	margin: 0 auto;
}

@keyframes spin {
	to {
		transform: rotate(360deg);
	}
}
</style>

