<template>
	<div class="workspace-invitations-list">
		<div class="flex items-center justify-between mb-4">
			<h4 class="text-md font-semibold">Workspace Invitations</h4>
		</div>

		<!-- Filter Tabs -->
		<div class="flex gap-2 mb-4 border-b dark:border-gray-700">
			<button
				v-for="filter in filters"
				:key="filter.value"
				@click="currentFilter = filter.value"
				:class="[
					'px-4 py-2 text-sm font-medium transition-colors border-b-2',
					currentFilter === filter.value
						? 'border-blue-500 text-blue-600 dark:text-blue-400'
						: 'border-transparent text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'
				]"
			>
				{{ filter.label }} ({{ getFilteredCount(filter.value) }})
			</button>
		</div>

		<div v-if="loading" class="text-center py-4">
			<div class="spinner"></div>
			<p class="mt-2 text-gray-600">Loading invitations...</p>
		</div>

		<div v-else-if="error" class="text-center py-4 text-red-500">
			<p>Error: {{ error }}</p>
			<Button @click="loadInvitations" class="mt-2">Try again</Button>
		</div>

		<div v-else-if="filteredInvitations.length === 0" class="text-center py-8 text-gray-500">
			<p>No {{ currentFilter !== 'all' ? currentFilter : '' }} invitations</p>
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="invitation in filteredInvitations"
				:key="invitation.id"
				:class="[
					'flex items-center justify-between p-3 border rounded-lg dark:border-gray-700',
					invitation.is_accepted ? 'bg-gray-50 dark:bg-gray-800 opacity-75' : '',
					isExpired(invitation) ? 'bg-red-50 dark:bg-red-900/20 opacity-75' : ''
				]"
			>
				<div class="flex-1">
					<div class="flex items-center gap-2 mb-1">
						<p class="font-medium">
							{{ invitation.email || 'General invitation' }}
						</p>
						<span 
							v-if="invitation.is_accepted" 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300"
						>
							✓ Accepted
						</span>
						<span 
							v-else-if="isExpired(invitation)" 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300"
						>
							✗ Expired
						</span>
						<span 
							v-else 
							class="px-2 py-0.5 text-xs font-medium rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300"
						>
							⏳ Pending
						</span>
					</div>
					<p class="text-sm text-gray-600 dark:text-gray-400 mb-1">
						<span class="font-medium">Invited by:</span> {{ invitation.user?.name || 'Unknown' }}
					</p>
					<p class="text-xs text-gray-500">
						Token: {{ invitation.token.substring(0, 8) }}...
					</p>
					<p class="text-xs text-gray-400">
						<span v-if="invitation.expired_at">
							Expires: {{ formatDate(invitation.expired_at) }}
						</span>
						<span v-else class="text-green-500">No expiration</span>
					</p>
				</div>

				<div class="flex gap-2">
					<Button
						v-if="invitation.email && !invitation.is_accepted && !isExpired(invitation)"
						variant="outline"
						size="sm"
						@click="handleResend(invitation.id)"
						:disabled="resending === invitation.id"
						title="Resend invitation email"
					>
						{{ resending === invitation.id ? 'Sending...' : 'Resend' }}
					</Button>
					<Button
						variant="destructive"
						size="sm"
						@click="handleRevoke(invitation.id)"
						:disabled="revoking === invitation.id || invitation.is_accepted || isExpired(invitation)"
						:title="isExpired(invitation) ? 'Cannot revoke expired invitation' : invitation.is_accepted ? 'Cannot revoke accepted invitation' : 'Revoke invitation'"
					>
						{{ revoking === invitation.id ? 'Revoking...' : 'Revoke' }}
					</Button>
				</div>
			</div>
		</div>
	</div>

	<!-- Revoke Confirm Dialog -->
	<Confirm
		v-if="showRevokeConfirm"
		title="Revoke invitation"
		body="Are you sure you want to revoke this invitation?"
		@on-ok="confirmRevoke"
		@on-cancel="cancelRevoke"
	/>
</template>

<script setup lang="ts">
	import { ref, onMounted, computed, watch } from 'vue';
	import { Button } from '@/components/ui/button';
	import Confirm from '@/components/general/Confirm.vue';
	import { getWorkspaceInvitations, revokeInvitation, resendInvitation, type WorkspaceInvitation } from '@/actions/tmgr/invitations';
	import { useToast } from '@/components/ui/toast';
	import { CircleCheckBigIcon } from 'lucide-vue-next';
	import store from '@/store';

	const toaster = useToast();
	const invitations = ref<WorkspaceInvitation[]>([]);
	const loading = ref(false);
	const error = ref<string | null>(null);
	const revoking = ref<number | null>(null);
	const resending = ref<number | null>(null);
	const currentFilter = ref<'all' | 'active' | 'expired' | 'accepted'>('all');
	const showRevokeConfirm = ref(false);
	const invitationToRevoke = ref<number | null>(null);

	const filters = [
		{ label: 'All', value: 'all' as const },
		{ label: 'Active', value: 'active' as const },
		{ label: 'Expired', value: 'expired' as const },
		{ label: 'Accepted', value: 'accepted' as const }
	];

	const currentWorkspaceId = computed(() => {
		const currentWorkspaceSetting = store.state.user?.settings?.find(
			(setting: any) => setting.key === 'current_workspace'
		);
		return currentWorkspaceSetting?.value;
	});

	const isExpired = (invitation: WorkspaceInvitation) => {
		if (!invitation.expired_at) return false;
		return new Date(invitation.expired_at) < new Date();
	};

	const filteredInvitations = computed(() => {
		switch (currentFilter.value) {
			case 'active':
				return invitations.value.filter(inv => !inv.is_accepted && !isExpired(inv));
			case 'expired':
				return invitations.value.filter(inv => isExpired(inv) && !inv.is_accepted);
			case 'accepted':
				return invitations.value.filter(inv => inv.is_accepted);
			default:
				return invitations.value;
		}
	});

	const getFilteredCount = (filterValue: string) => {
		switch (filterValue) {
			case 'active':
				return invitations.value.filter(inv => !inv.is_accepted && !isExpired(inv)).length;
			case 'expired':
				return invitations.value.filter(inv => isExpired(inv) && !inv.is_accepted).length;
			case 'accepted':
				return invitations.value.filter(inv => inv.is_accepted).length;
			default:
				return invitations.value.length;
		}
	};

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

	const handleRevoke = (invitationId: number) => {
		if (!currentWorkspaceId.value) return;
		invitationToRevoke.value = invitationId;
		showRevokeConfirm.value = true;
	};

	const confirmRevoke = async () => {
		if (!currentWorkspaceId.value || !invitationToRevoke.value) return;

		revoking.value = invitationToRevoke.value;

		try {
			await revokeInvitation(Number(currentWorkspaceId.value), invitationToRevoke.value);
			invitations.value = invitations.value.filter(inv => inv.id !== invitationToRevoke.value);
			
			toaster.toast({
				title: 'Invitation revoked',
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (err: any) {
			const errorMessage = err.response?.data?.message || 'Failed to revoke invitation';
			error.value = errorMessage;
			toaster.toast({
				title: 'Error',
				description: errorMessage,
				variant: 'destructive',
			});
			console.error('Error revoking invitation:', err);
		} finally {
			revoking.value = null;
			showRevokeConfirm.value = false;
			invitationToRevoke.value = null;
		}
	};

	const cancelRevoke = () => {
		showRevokeConfirm.value = false;
		invitationToRevoke.value = null;
	};

	const handleResend = async (invitationId: number) => {
		if (!currentWorkspaceId.value) return;

		resending.value = invitationId;

		try {
			await resendInvitation(Number(currentWorkspaceId.value), invitationId);
			
			toaster.toast({
				title: 'Invitation resent',
				description: 'Email has been sent successfully',
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (err: any) {
			const errorMessage = err.response?.data?.message || 'Failed to resend invitation';
			error.value = errorMessage;
			toaster.toast({
				title: 'Error',
				description: errorMessage,
				variant: 'destructive',
			});
			console.error('Error resending invitation:', err);
		} finally {
			resending.value = null;
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

	onMounted(() => {
		loadInvitations();
	});

	watch(currentWorkspaceId, (newValue, oldValue) => {
		if (newValue && newValue !== oldValue) {
			loadInvitations();
		}
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

