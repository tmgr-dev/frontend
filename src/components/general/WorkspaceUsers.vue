<template>
	<div class="flex items-center">
		<div class="-space-x-2 flex">
			<AppTooltip
				v-for="user in users"
				:key="user.id"
				:content="
					$t('general.workspaceUsers.viewAllMembers', { name: user.name })
				"
				side="bottom"
			>
				<button
					type="button"
					:aria-label="
						$t('general.workspaceUsers.memberLabel', { name: user.name })
					"
					:class="[
						'relative flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-full border-2 border-surface shadow-tmgr-xs transition-all hover:z-20 hover:scale-110',
						avatarColor(user.id),
					]"
					@click="showMembersModal = true"
				>
					<UserAvatar
						:user-id="user.id"
						:name="user.name"
						:has-avatar="user.has_avatar ?? false"
						:size="32"
						class="!bg-transparent !text-inherit"
					/>
				</button>
			</AppTooltip>

			<Dialog v-if="!isLocalWorkspace" v-model:open="isInviteDialogOpen">
				<AppTooltip
					:content="$t('general.workspaceUsers.inviteUser')"
					side="bottom"
				>
					<DialogTrigger as-child>
						<button
							:aria-label="$t('general.workspaceUsers.inviteUser')"
							class="relative flex h-8 w-8 shrink-0 cursor-pointer items-center justify-center rounded-full border-2 border-surface bg-surface-sunken text-ink-subtle shadow-tmgr-xs transition-all hover:z-20 hover:scale-110 hover:text-ink"
						>
							<span class="material-icons text-sm">add</span>
						</button>
					</DialogTrigger>
				</AppTooltip>

				<DialogContent class="!rounded-[8px] sm:max-w-[425px]">
					<DialogHeader>
						<DialogTitle>{{
							$t('general.workspaceUsers.sendInvitations')
						}}</DialogTitle>
					</DialogHeader>

					<Textarea
						v-model="invitationEmails"
						rows="5"
						:placeholder="$t('general.workspaceUsers.emailsPlaceholder')"
					/>
					<span
						class="whitespace-pre text-status-fix-fg"
						v-if="
							!invitationEmailsValidationError.isValid &&
							invitationEmails !== ''
						"
					>
						{{ invitationEmailsValidationError.errors?.join(`\n`) }}
					</span>
					<DialogFooter>
						<Button
							variant="default"
							type="submit"
							:disabled="
								!invitationEmailsValidationError.isValid ||
								invitationEmails.trim() === '' ||
								isSending
							"
							@click="sendInvitations"
						>
							{{
								isSending
									? $t('general.workspaceUsers.sending')
									: $t('general.workspaceUsers.send')
							}}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>

		<WorkspaceMembersModal
			v-model="showMembersModal"
			:workspace-id="workspaceId"
			@member-removed="handleMemberRemoved"
		/>
	</div>
</template>

<script setup lang="ts">
	import { t } from '@/i18n';
	import { createWorkspaceInvitation } from '@/actions/tmgr/workspaces';
	import AppTooltip from '@/components/general/AppTooltip.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import { Textarea } from '@/components/ui/textarea';
	import { useToast } from '@/components/ui/toast';
	import WorkspaceMembersModal from '@/components/workspace/WorkspaceMembersModal.vue';
	import { errorMessageFrom } from '@/utils/emailVerification';
	import { validateEmailString, ValidationResult } from '@/utils/emails';
	import { computed, ref } from 'vue';

	export interface WorkspaceUser {
		has_avatar?: boolean;
		id: number;
		name: string;
	}

	interface Props {
		users: WorkspaceUser[];
		workspaceId: number;
	}

	const props = defineProps<Props>();
	const isLocalWorkspace = computed(() => props.workspaceId < 0);
	const toaster = useToast();

	const isInviteDialogOpen = ref(false);
	const showMembersModal = ref(false);
	const invitationEmails = ref('');
	const isSending = ref(false);
	const invitationEmailsValidationError = ref<ValidationResult>({
		isValid: true,
		errors: [],
	});

	async function sendInvitations() {
		invitationEmailsValidationError.value = {
			isValid: true,
			errors: [],
		};
		const emails = invitationEmails.value.trim().replaceAll('\n', '');
		if (emails === '') return;

		const validationResult = validateEmailString(emails);

		if (!validationResult.isValid) {
			invitationEmailsValidationError.value = validationResult;
			return;
		}

		try {
			isSending.value = true;
			await createWorkspaceInvitation(props.workspaceId, { emails });
			toaster.toast({
				title: t('general.workspaceUsers.invitationsSent'),
				description: t('general.workspaceUsers.invitationsSentDescription'),
			});
			isInviteDialogOpen.value = false;
			invitationEmails.value = '';
		} catch (error) {
			console.error('Failed to send invitations:', error);
			toaster.toast({
				title: t('general.workspaceUsers.errorTitle'),
				description: errorMessageFrom(
					error,
					t('general.workspaceUsers.sendFailed'),
				),
				variant: 'destructive',
			});
		} finally {
			isSending.value = false;
		}
	}

	function handleMemberRemoved(memberId: number) {
		console.log('Member removed:', memberId);
	}

	const avatarPalette = [
		'bg-status-fix-bg text-status-fix-fg',
		'bg-status-progress-bg text-status-progress-fg',
		'bg-status-testing-bg text-status-testing-fg',
		'bg-status-done-bg text-status-done-fg',
		'bg-status-todo-bg text-status-todo-fg',
		'bg-brand-bg text-brand-fg',
	];

	function avatarColor(id: number): string {
		const idx = Math.abs(id) % avatarPalette.length;
		return avatarPalette[idx];
	}
</script>
