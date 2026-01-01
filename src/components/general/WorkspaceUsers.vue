<template>
	<div class="flex items-center">
		<div class="flex -space-x-2">
			<button
				v-for="user in users"
				:key="user.id"
				type="button"
				:class="[
					'relative flex h-8 w-8 shrink-0 items-center justify-center rounded-full border-2 border-white dark:border-gray-800 bg-gradient-to-br from-blue-500 to-blue-600 shadow-md transition-all hover:scale-110 hover:z-20 cursor-pointer',
				]"
				v-tooltip.bottom="`${user.name} - Click to view all members`"
				@click="showMembersModal = true"
			>
				<span class="text-sm font-medium text-white">
					{{ user.name.charAt(0).toUpperCase() }}
				</span>
			</button>
			
			<Dialog v-model:open="isInviteDialogOpen">
				<DialogTrigger as-child>
					<button
						class="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-full border-2 border-white dark:border-gray-800 bg-gradient-to-br from-gray-400 to-gray-500 dark:from-gray-600 dark:to-gray-700 shadow-md transition-all hover:scale-110 hover:z-20 cursor-pointer"
						v-tooltip.bottom="'Invite user'"
					>
						<span class="material-icons text-sm text-white">add</span>
					</button>
				</DialogTrigger>

			<DialogContent
				class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
			>
				<DialogHeader>
					<DialogTitle>Send invitation(s)</DialogTitle>
				</DialogHeader>

				<Textarea
					v-model="invitationEmails"
					rows="5"
					:placeholder="`Enter emails (comma separated), for example:\nuser1@example.com,\nuser2@example.com,\n...\nuserN@example.com`"
				/>
				<span class="text-red-500 whitespace-pre" v-if="!invitationEmailsValidationError.isValid && invitationEmails !== ''">
					{{ invitationEmailsValidationError.errors?.join(`\n`) }}
				</span>
				<DialogFooter>
					<Button
						variant="default"
						type="submit"
						:disabled="!invitationEmailsValidationError.isValid || invitationEmails.trim() === '' || isSending"
						@click="sendInvitations"
					>
						{{ isSending ? 'Sending...' : 'Send' }}
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
	import { ref } from 'vue';
	import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
	import { Button } from '@/components/ui/button';
	import { Textarea } from '@/components/ui/textarea';
	import { createWorkspaceInvitation } from '@/actions/tmgr/workspaces';
	import { validateEmailString, ValidationResult } from '@/utils/emails';
	import { useToast } from '@/components/ui/toast';
	import WorkspaceMembersModal from '@/components/workspace/WorkspaceMembersModal.vue';

	export interface WorkspaceUser {
		id: number;
		name: string;
	}

	interface Props {
		users: WorkspaceUser[];
		workspaceId: number;
	}

	const props = defineProps<Props>();
	const toaster = useToast();
	
	const isInviteDialogOpen = ref(false);
	const showMembersModal = ref(false);
	const invitationEmails = ref('');
	const isSending = ref(false);
	const invitationEmailsValidationError = ref<ValidationResult>({
		isValid: true,
		errors: []
	});

	async function sendInvitations() {
		invitationEmailsValidationError.value = {
			isValid: true,
			errors: []
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
				title: 'Invitations sent',
				description: 'Invitations have been sent successfully',
			});
			isInviteDialogOpen.value = false;
			invitationEmails.value = '';
		} catch (error) {
			console.error('Failed to send invitations:', error);
			toaster.toast({
				title: 'Error',
				description: 'Failed to send invitations. Please try again.',
				variant: 'destructive',
			});
		} finally {
			isSending.value = false;
		}
	}

	function handleMemberRemoved(memberId: number) {
		console.log('Member removed:', memberId);
	}
</script>

