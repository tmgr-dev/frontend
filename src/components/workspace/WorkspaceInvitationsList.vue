<template>
	<div class="workspace-invitations-list">
		<!-- Filter Tabs -->
		<div class="mb-4 inline-flex gap-1 rounded-md bg-muted p-1">
			<button
				v-for="filter in filters"
				:key="filter.value"
				@click="currentFilter = filter.value"
				:class="[
					'rounded-sm px-3 py-1.5 text-sm font-medium transition-colors',
					currentFilter === filter.value
						? 'bg-background text-ink shadow-xs'
						: 'text-ink-subtle hover:text-ink',
				]"
			>
				{{ $t(filter.labelKey) }} ({{ getFilteredCount(filter.value) }})
			</button>
		</div>

		<div v-if="loading" class="py-4 text-center">
			<div class="spinner"></div>
			<p class="mt-2 text-ink-subtle">
				{{ $t('workspace.workspaceInvitationsList.loading') }}
			</p>
		</div>

		<div v-else-if="error" class="py-4 text-center text-destructive">
			<p>
				{{
					$t('workspace.workspaceInvitationsList.errorLine', { message: error })
				}}
			</p>
			<Button @click="loadInvitations" class="mt-2">{{
				$t('workspace.workspaceInvitationsList.tryAgain')
			}}</Button>
		</div>

		<div
			v-else-if="filteredInvitations.length === 0"
			class="py-8 text-center text-sm text-ink-subtle"
		>
			<p>{{ $t(EMPTY_KEYS[currentFilter]) }}</p>
		</div>

		<div v-else class="space-y-3">
			<div
				v-for="invitation in filteredInvitations"
				:key="invitation.id"
				:class="[
					'flex items-center justify-between rounded-lg border border-border p-3',
					invitation.is_accepted ? 'bg-muted/50 opacity-75' : '',
					isExpired(invitation) ? 'bg-destructive/10 opacity-75' : '',
				]"
			>
				<div class="flex-1">
					<div class="mb-1 flex items-center gap-2">
						<p class="font-medium text-ink">
							{{
								invitation.email ||
								$t('workspace.workspaceInvitationsList.general')
							}}
						</p>
						<span
							v-if="invitation.is_accepted"
							class="rounded-full bg-emerald-100 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400"
						>
							{{ $t('workspace.workspaceInvitationsList.accepted') }}
						</span>
						<span
							v-else-if="isExpired(invitation)"
							class="rounded-full bg-destructive/10 px-2 py-0.5 text-xs font-medium text-destructive"
						>
							{{ $t('workspace.workspaceInvitationsList.expired') }}
						</span>
						<span
							v-else
							class="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground"
						>
							{{ $t('workspace.workspaceInvitationsList.pending') }}
						</span>
					</div>
					<p class="mb-1 text-sm text-ink-subtle">
						<span class="font-medium">{{
							$t('workspace.workspaceInvitationsList.invitedBy')
						}}</span>
						{{
							invitation.user?.name ||
							$t('workspace.workspaceInvitationsList.unknown')
						}}
					</p>
					<p class="text-xs text-ink-subtle">
						{{
							$t('workspace.workspaceInvitationsList.token', {
								token: invitation.token.substring(0, 8),
							})
						}}
					</p>
					<p class="text-xs text-ink-subtle">
						<span v-if="invitation.expired_at">
							{{
								$t('workspace.workspaceInvitationsList.expires', {
									date: formatDate(invitation.expired_at),
								})
							}}
						</span>
						<span v-else class="text-emerald-600 dark:text-emerald-400">{{
							$t('workspace.workspaceInvitationsList.noExpiration')
						}}</span>
					</p>
				</div>

				<div class="flex gap-2">
					<Button
						v-if="
							invitation.email &&
							!invitation.is_accepted &&
							!isExpired(invitation)
						"
						variant="outline"
						size="sm"
						@click="handleResend(invitation.id)"
						:disabled="resending === invitation.id"
						:title="$t('workspace.workspaceInvitationsList.resendTitle')"
					>
						{{
							resending === invitation.id
								? $t('workspace.workspaceInvitationsList.sending')
								: $t('workspace.workspaceInvitationsList.resend')
						}}
					</Button>
					<Button
						variant="destructive"
						size="sm"
						@click="handleRevoke(invitation.id)"
						:disabled="
							revoking === invitation.id ||
							invitation.is_accepted ||
							isExpired(invitation)
						"
						:title="
							isExpired(invitation)
								? $t('workspace.workspaceInvitationsList.cannotRevokeExpired')
								: invitation.is_accepted
									? $t(
											'workspace.workspaceInvitationsList.cannotRevokeAccepted',
										)
									: $t('workspace.workspaceInvitationsList.revokeTitle')
						"
					>
						{{
							revoking === invitation.id
								? $t('workspace.workspaceInvitationsList.revoking')
								: $t('workspace.workspaceInvitationsList.revoke')
						}}
					</Button>
				</div>
			</div>
		</div>
	</div>

	<!-- Revoke Confirm Dialog -->
	<Confirm
		v-if="showRevokeConfirm"
		:title="$t('workspace.workspaceInvitationsList.revokeTitle')"
		:body="$t('workspace.workspaceInvitationsList.confirmBody')"
		@on-ok="confirmRevoke"
		@on-cancel="cancelRevoke"
	/>
</template>

<script setup lang="ts">
	import { currentLocale, t } from '@/i18n';
	import {
		getWorkspaceInvitations,
		resendInvitation,
		revokeInvitation,
		type WorkspaceInvitation,
	} from '@/actions/tmgr/invitations';
	import Confirm from '@/components/general/Confirm.vue';
	import { Button } from '@/components/ui/button';
	import { useToast } from '@/components/ui/toast';
	import store from '@/store';
	import { CircleCheckBigIcon } from 'lucide-vue-next';
	import { computed, onMounted, ref, watch } from 'vue';

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
		{
			labelKey: 'workspace.workspaceInvitationsList.filterAll',
			value: 'all' as const,
		},
		{
			labelKey: 'workspace.workspaceInvitationsList.filterActive',
			value: 'active' as const,
		},
		{
			labelKey: 'workspace.workspaceInvitationsList.filterExpired',
			value: 'expired' as const,
		},
		{
			labelKey: 'workspace.workspaceInvitationsList.filterAccepted',
			value: 'accepted' as const,
		},
	];

	const EMPTY_KEYS = {
		all: 'workspace.workspaceInvitationsList.emptyAll',
		active: 'workspace.workspaceInvitationsList.emptyActive',
		expired: 'workspace.workspaceInvitationsList.emptyExpired',
		accepted: 'workspace.workspaceInvitationsList.emptyAccepted',
	};

	const currentWorkspaceId = computed(() => {
		const currentWorkspaceSetting = store.state.user?.settings?.find(
			(setting: any) => setting.key === 'current_workspace',
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
				return invitations.value.filter(
					(inv) => !inv.is_accepted && !isExpired(inv),
				);
			case 'expired':
				return invitations.value.filter(
					(inv) => isExpired(inv) && !inv.is_accepted,
				);
			case 'accepted':
				return invitations.value.filter((inv) => inv.is_accepted);
			default:
				return invitations.value;
		}
	});

	const getFilteredCount = (filterValue: string) => {
		switch (filterValue) {
			case 'active':
				return invitations.value.filter(
					(inv) => !inv.is_accepted && !isExpired(inv),
				).length;
			case 'expired':
				return invitations.value.filter(
					(inv) => isExpired(inv) && !inv.is_accepted,
				).length;
			case 'accepted':
				return invitations.value.filter((inv) => inv.is_accepted).length;
			default:
				return invitations.value.length;
		}
	};

	const loadInvitations = async () => {
		if (!currentWorkspaceId.value) return;

		loading.value = true;
		error.value = null;

		try {
			invitations.value = await getWorkspaceInvitations(
				Number(currentWorkspaceId.value),
			);
		} catch (err: any) {
			error.value =
				err.response?.data?.message ||
				t('workspace.workspaceInvitationsList.loadFailed');
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
			await revokeInvitation(
				Number(currentWorkspaceId.value),
				invitationToRevoke.value,
			);
			invitations.value = invitations.value.filter(
				(inv) => inv.id !== invitationToRevoke.value,
			);

			toaster.toast({
				title: t('workspace.workspaceInvitationsList.revoked'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (err: any) {
			const errorMessage =
				err.response?.data?.message ||
				t('workspace.workspaceInvitationsList.revokeFailed');
			error.value = errorMessage;
			toaster.toast({
				title: t('workspace.workspaceInvitationsList.errorTitle'),
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
				title: t('workspace.workspaceInvitationsList.resent'),
				description: t('workspace.workspaceInvitationsList.resentDescription'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (err: any) {
			const errorMessage =
				err.response?.data?.message ||
				t('workspace.workspaceInvitationsList.resendFailed');
			error.value = errorMessage;
			toaster.toast({
				title: t('workspace.workspaceInvitationsList.errorTitle'),
				description: errorMessage,
				variant: 'destructive',
			});
			console.error('Error resending invitation:', err);
		} finally {
			resending.value = null;
		}
	};

	const formatDate = (dateString: string | null) => {
		if (!dateString) return t('workspace.workspaceInvitationsList.never');
		const date = new Date(dateString);
		return date.toLocaleDateString(currentLocale(), {
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
