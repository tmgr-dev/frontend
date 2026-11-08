<script setup lang="ts">
	import AsyncContent from '@/components/async/AsyncContent.vue';

	import { FormSetting } from '@/actions/tmgr/settings.ts';
	import {
		getUserSettings,
		getUserSettingsV2,
		updateUserSettingsV2,
	} from '@/actions/tmgr/user';
	import {
		createWorkspace as createWorkspaceAction,
		createWorkspaceInvitation as createWorkspaceInvitationAction,
		deleteWorkspace as deleteWorkspaceAction,
		exitWorkspace as exitWorkspaceAction,
		getWorkspaceMembers,
		getWorkspaces,
		removeMemberFromWorkspace,
		updateWorkspace as updateWorkspaceAction,
		Workspace,
		type WorkspaceMember,
	} from '@/actions/tmgr/workspaces';
	import Combobox from '@/components/Combobox.vue';
	import ConfirmByNameDialog from '@/components/general/ConfirmByNameDialog.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
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
		AlertDialogTrigger,
	} from '@/components/ui/alert-dialog';
	import { Avatar, AvatarFallback } from '@/components/ui/avatar';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Switch } from '@/components/ui/switch';
	import { Textarea } from '@/components/ui/textarea';
	import { useToast } from '@/components/ui/toast';
	import WorkspaceInvitationsList from '@/components/workspace/WorkspaceInvitationsList.vue';
	import { dialogState } from '@/composable/dialog.ts';
	import { t } from '@/i18n';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import store from '@/store';
	import { validateEmailString, ValidationResult } from '@/utils/emails.ts';
	import { errorMessageFrom, httpErrorInfo } from '@/utils/emailVerification';
	import { serverMessageFrom } from '@/utils/serverError';
	import { convertToHHMM, timeToSeconds } from '@/utils/timeUtils';
	import {
		normalizeWorkspaceColor,
		WORKSPACE_COLORS,
		workspaceIconStyle,
	} from '@/utils/workspaceColor';
	import {
		CheckIcon,
		CircleCheckBigIcon,
		FolderKanbanIcon,
		LogOutIcon,
		PackageOpen,
		SaveIcon,
		Trash2Icon,
		UserPlus,
	} from 'lucide-vue-next';
	import {
		computed,
		onBeforeMount,
		onBeforeUnmount,
		Ref,
		ref,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const toaster = useToast();
	const [isOpen, closeDialog] = dialogState();
	const [isOpenInvitation] = dialogState();
	const newWorkspace = ref({
		name: '',
		type: 'test',
	});
	const invitationEmails = ref('');
	const invitationEmailsValidationError = ref<ValidationResult>({
		isValid: true,
		errors: [],
	});

	const route = useRoute();
	const router = useRouter();
	const settings = ref();
	const workspaces: Ref<Workspace[]> = ref([]);
	const activeWorkspace = ref<FormSetting>();
	const activeWorkspaceType = computed(() => {
		const foundWorkspace = workspaces.value.find(
			(workspace) => workspace.id == activeWorkspace.value?.value,
		);

		return foundWorkspace?.type;
	});
	const activeWorkspaceName = computed(
		() =>
			workspaces.value.find(
				(workspace) => workspace.id == activeWorkspace.value?.value,
			)?.name ?? '',
	);
	const activeWorkspaceItem = computed(() =>
		workspaces.value.find(
			(workspace) => workspace.id == activeWorkspace.value?.value,
		),
	);
	const workspaceName = ref('');
	const workspaceColor = ref('');
	const savingWorkspace = ref(false);
	const trimmedWorkspaceName = computed(() => workspaceName.value.trim());
	const workspaceDirty = computed(
		() =>
			trimmedWorkspaceName.value !== activeWorkspaceItem.value?.name ||
			workspaceColor.value !==
				normalizeWorkspaceColor(activeWorkspaceItem.value?.color),
	);
	watch(
		activeWorkspaceItem,
		(workspace) => {
			workspaceName.value = workspace?.name ?? '';
			workspaceColor.value = normalizeWorkspaceColor(workspace?.color);
		},
		{ immediate: true },
	);
	const isDeleteDialogOpen = ref(false);
	const isDeleting = ref(false);
	const isLocalActiveWorkspace = computed(
		() => Number(activeWorkspace.value?.value) < 0,
	);
	const isOwnedActiveWorkspace = computed(() => {
		const foundWorkspace = workspaces.value.find(
			(workspace) => workspace.id == activeWorkspace.value?.value,
		);

		return (
			foundWorkspace?.user_id != null &&
			Number(foundWorkspace.user_id) === Number(store.state.user?.id)
		);
	});
	const defaultWorkspaceSelection = ref<number | null>(null);
	const savingDefaultWorkspace = ref(false);
	const cloudWorkspaceOptions = computed(() =>
		workspaces.value.filter((workspace) => !workspace.is_local),
	);
	const isLoading = ref(true);
	const savingSettings = ref(false);
	const initialLoaded = ref(false),
		initialError = ref<string | null>(null);
	const membersLoaded = ref(false),
		membersError = ref<string | null>(null);
	let disposed = false,
		memberRequest = 0;
	onBeforeUnmount(() => {
		disposed = true;
		++memberRequest;
	});
	const members = ref<WorkspaceMember[]>([]);
	const loadingMembers = ref(false);
	const removingMemberId = ref<number | null>(null);
	const memberToRemove = ref<WorkspaceMember | null>(null);
	const showRemoveMemberDialog = ref(false);

	function openCreateWorkspaceModalIfQueryParamHasCreateKey(query: any) {
		if (query.hasOwnProperty('create')) {
			router.push(route.path);
		}
	}

	function openInviteModalIfQueryParamHasInviteKey(query: any) {
		if (query.hasOwnProperty('invite')) {
			router.push(route.path);
		}
	}

	watch(isOpen, (newValue) => {
		if (!newValue) {
			openCreateWorkspaceModalIfQueryParamHasCreateKey(route.query);
		}
	});

	watch(isOpenInvitation, (newValue) => {
		if (!newValue) {
			openInviteModalIfQueryParamHasInviteKey(route.query);
		}
	});

	watch(
		route,
		(to) => {
			if (!isOpen.value && to.query.hasOwnProperty('create')) {
				isOpen.value = true;
			}
			if (!isOpenInvitation.value && to.query.hasOwnProperty('invite')) {
				isOpenInvitation.value = true;
			}
		},
		{
			flush: 'pre',
			immediate: true,
			deep: true,
		},
	);

	watch(invitationEmails, (newValue) => {
		invitationEmailsValidationError.value = validateEmailString(newValue);
	});

	async function loadWorkspaceSettings() {
		isLoading.value = true;
		initialError.value = null;
		try {
			setDocumentTitle(t('settingsPages.workspaces.documentTitle'));
			const [loadedWorkspaces, loadedSettings] = await Promise.all([
				getWorkspaces(),
				getUserSettingsV2(),
			]);
			if (disposed) return;
			isOpen.value = route.query.hasOwnProperty('create');
			isOpenInvitation.value = route.query.hasOwnProperty('invite');
			isLoading.value = false;
			workspaces.value = loadedWorkspaces;
			activeWorkspace.value = store.state.user?.settings?.find(
				(settingInStore) => settingInStore.key === 'current_workspace',
			);
			defaultWorkspaceSelection.value = store.state.defaultWorkspaceId;

			const mappedSettingWithUserSettings = loadedSettings.map((setting) => {
				const settingFromStoreWithValue = store.state.user?.settings?.find(
					(settingInStore) => settingInStore.id === setting.id,
				);
				return settingFromStoreWithValue
					? {
							...setting,
							...settingFromStoreWithValue,
						}
					: {
							...setting,
							value: null,
						};
			});

			settings.value = mappedSettingWithUserSettings.filter(
				(setting) => setting.key !== 'current_workspace',
			);
			initialLoaded.value = true;
		} catch {
			if (!disposed)
				initialError.value = t('settingsPages.workspaces.loadFailed');
		} finally {
			if (!disposed) isLoading.value = false;
		}
	}
	onBeforeMount(loadWorkspaceSettings);

	async function updateSettings() {
		if (savingSettings.value) return;
		savingSettings.value = true;
		try {
			const updatedSettings = [...settings.value, activeWorkspace.value];

			// Store original editor value to check if it changes
			const preferredEditorSetting = settings.value.find(
				(setting: any) => setting.key === 'preferred_editor',
			);
			const originalEditorValue = preferredEditorSetting?.value;

			// Always update localStorage if preferred_editor setting exists
			// This ensures the value is consistently saved
			if (preferredEditorSetting) {
				// Make sure we convert to string and handle null/undefined values
				let editorValue = preferredEditorSetting.value
					? String(preferredEditorSetting.value).toLowerCase().trim()
					: '';

				// Validate the editor value
				if (
					editorValue &&
					(editorValue === 'block' || editorValue === 'markdown')
				) {
					console.log('Saving editor preference to localStorage:', editorValue);
					localStorage.setItem('preferred_editor', editorValue);

					// Also update the editor value in the Vuex store
					if (store.state.user && store.state.user.settings) {
						// Update the store's user settings directly
						store.state.user.settings = store.state.user.settings.map(
							(setting: any) => {
								if (setting.key === 'preferred_editor') {
									return { ...setting, value: editorValue };
								}
								return setting;
							},
						);
					}
				} else if (editorValue) {
					console.error('Invalid editor value:', editorValue);
				}
			}

			await updateUserSettingsV2(updatedSettings);

			// If editor setting has changed, reload the page to apply changes immediately
			if (preferredEditorSetting) {
				const normalizedNewValue = preferredEditorSetting.value
					? String(preferredEditorSetting.value).toLowerCase().trim()
					: '';

				const normalizedOriginalValue = originalEditorValue
					? String(originalEditorValue).toLowerCase().trim()
					: '';

				if (
					normalizedNewValue &&
					normalizedNewValue !== normalizedOriginalValue
				) {
					toaster.toast({
						variant: 'default',
						title: t('settingsPages.workspaces.toast.editorUpdated'),
						action: CircleCheckBigIcon,
						class: 'bg-green-500 border-0 text-white',
					});

					// Force localStorage update one more time to ensure it's saved before reload
					if (
						normalizedNewValue === 'block' ||
						normalizedNewValue === 'markdown'
					) {
						localStorage.setItem('preferred_editor', normalizedNewValue);
					}

					setTimeout(() => {
						window.location.reload();
					}, 500);
					return;
				}
			}

			toaster.toast({
				variant: 'default',
				title: t('settingsPages.workspaces.toast.saved'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (e) {
			console.error(e);
			toaster.toast({
				title: t('settingsPages.workspaces.toast.saveSettingsFailed'),
				variant: 'destructive',
			});
		} finally {
			savingSettings.value = false;
		}
	}

	async function saveWorkspace() {
		const current = activeWorkspaceItem.value;
		if (
			!current ||
			savingWorkspace.value ||
			trimmedWorkspaceName.value === '' ||
			!workspaceDirty.value
		)
			return;
		const payload: { name?: string; color?: string } = {};
		if (trimmedWorkspaceName.value !== current.name)
			payload.name = trimmedWorkspaceName.value;
		if (workspaceColor.value !== normalizeWorkspaceColor(current.color))
			payload.color = workspaceColor.value;
		savingWorkspace.value = true;
		try {
			const updated = await updateWorkspaceAction(current.id, payload);
			if (!disposed)
				workspaces.value = workspaces.value.map((workspace) =>
					workspace.id === updated.id ? updated : workspace,
				);
			toaster.toast({
				variant: 'default',
				title: t('settingsPages.workspaces.toast.workspaceSaved'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (e) {
			console.error(e);
			toaster.toast({
				title: t('settingsPages.workspaces.toast.error'),
				description: serverMessageFrom(
					e,
					t('settingsPages.workspaces.toast.saveWorkspaceFailed'),
				),
				variant: 'destructive',
			});
		} finally {
			savingWorkspace.value = false;
		}
	}

	async function saveDefaultWorkspace() {
		if (
			savingDefaultWorkspace.value ||
			!activeWorkspace.value ||
			defaultWorkspaceSelection.value == null
		)
			return;
		savingDefaultWorkspace.value = true;
		try {
			const payload = [
				...settings.value,
				{ ...activeWorkspace.value, value: defaultWorkspaceSelection.value },
			];
			const updatedUser = await updateUserSettingsV2(payload, {
				setDefaultWorkspace: true,
			});
			store.commit('setUser', updatedUser);
			toaster.toast({
				variant: 'default',
				title: t('settingsPages.workspaces.toast.defaultSaved'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (e) {
			console.error(e);
			toaster.toast({
				title: t('settingsPages.workspaces.toast.defaultSaveFailed'),
				variant: 'destructive',
			});
		} finally {
			savingDefaultWorkspace.value = false;
		}
	}

	async function createWorkspace() {
		if (newWorkspace.value.name.trim() === '') return;

		try {
			const created = await createWorkspaceAction(newWorkspace.value);
			newWorkspace.value.name = '';
			closeDialog();
			toaster.toast({
				title: t('settingsPages.workspaces.toast.created'),
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});

			workspaces.value = await getWorkspaces();
			await store.dispatch('loadWorkspaces');
			const workspace = workspaces.value.find((w) => w.id === created.id);
			if (workspace) {
				store.commit('updateUserWorkspaceSetting', {
					workspaceId: workspace.id,
				});
				activeWorkspace.value = store.state.user?.settings?.find(
					(settingInStore) => settingInStore.key === 'current_workspace',
				);
				store
					.dispatch('featureToggles/fetchWorkspaceModules', workspace.id)
					.catch(() => undefined);
			}
		} catch (e) {
			console.error(e);
		}
	}

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

		if (!activeWorkspace?.value?.value) {
			return;
		}

		const workspaceId = activeWorkspace.value.value;
		try {
			await createWorkspaceInvitationAction(
				typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId,
				{ emails },
			);
		} catch (error) {
			toaster.toast({
				title: t('settingsPages.workspaces.toast.error'),
				description: errorMessageFrom(
					error,
					t('settingsPages.workspaces.toast.inviteFailed'),
				),
				variant: 'destructive',
			});
			return;
		}
		isOpenInvitation.value = false;
		invitationEmails.value = '';
	}

	async function updateUserSettings() {
		await getUserSettings();
		setTimeout(() => {
			window.location.reload();
		}, 100);
	}

	async function deleteWorkspace(typedName: string) {
		const workspaceId = activeWorkspace.value?.value;
		if (!workspaceId || isDeleting.value) return;
		isDeleting.value = true;
		try {
			await deleteWorkspaceAction(
				typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId,
				typedName,
			);
			isDeleteDialogOpen.value = false;
			await updateUserSettings();
		} catch (error) {
			const { status, message } = httpErrorInfo(error);
			if (status === 422 && message) {
				workspaces.value = await getWorkspaces(false).catch(
					() => workspaces.value,
				);
			}
			toaster.toast({
				title: t('settingsPages.workspaces.toast.error'),
				description:
					status === 422 && message
						? message
						: errorMessageFrom(
								error,
								t('settingsPages.workspaces.toast.deleteFailed'),
							),
				variant: 'destructive',
			});
		} finally {
			isDeleting.value = false;
		}
	}

	async function exitWorkspace() {
		try {
			const workspaceId = activeWorkspace.value?.value;
			if (workspaceId) {
				await exitWorkspaceAction(
					typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId,
				);
				await updateUserSettings();
			}
		} catch (e) {
			console.error(e);
			toaster.toast({
				title: t('settingsPages.workspaces.toast.error'),
				description: serverMessageFrom(
					e,
					t('settingsPages.workspaces.toast.leaveFailed'),
				),
				variant: 'destructive',
			});
		}
	}

	async function loadMembers() {
		const workspaceId = activeWorkspace.value?.value;
		const request = ++memberRequest;
		if (!workspaceId) {
			loadingMembers.value = false;
			return;
		}
		loadingMembers.value = true;
		membersError.value = null;
		const current = () =>
			!disposed &&
			request === memberRequest &&
			workspaceId === activeWorkspace.value?.value;
		try {
			const result = await getWorkspaceMembers(Number(workspaceId));
			if (current()) {
				members.value = result;
				membersLoaded.value = true;
			}
		} catch {
			if (current())
				membersError.value = t('settingsPages.workspaces.membersLoadFailed');
		} finally {
			if (current()) loadingMembers.value = false;
		}
	}

	function getInitials(name: string) {
		return name
			.split(' ')
			.map((part) => part.charAt(0))
			.join('')
			.toUpperCase()
			.slice(0, 2);
	}

	function canRemoveMember(member: WorkspaceMember) {
		const workspaceId = activeWorkspace.value?.value;
		const workspace = workspaces.value.find((w) => w.id == workspaceId);
		return (
			workspace?.user_id === store.state.user.id &&
			member.id !== workspace?.user_id
		);
	}

	function handleRemoveMember(member: WorkspaceMember) {
		memberToRemove.value = member;
		showRemoveMemberDialog.value = true;
	}

	async function confirmRemoveMember() {
		if (!memberToRemove.value) return;

		const workspaceId = activeWorkspace.value?.value;
		if (!workspaceId) return;

		removingMemberId.value = memberToRemove.value.id;
		showRemoveMemberDialog.value = false;

		try {
			const parsedWorkspaceId =
				typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId;
			await removeMemberFromWorkspace(
				parsedWorkspaceId,
				memberToRemove.value.id,
			);

			toaster.toast({
				title: t('settingsPages.workspaces.toast.memberRemoved'),
				description: t(
					'settingsPages.workspaces.toast.memberRemovedDescription',
					{
						name: memberToRemove.value.name,
					},
				),
			});

			await loadMembers();
		} catch (e: any) {
			console.error('Error removing member:', e);
			toaster.toast({
				title: t('settingsPages.workspaces.toast.error'),
				description:
					e.response?.data?.message ||
					t('settingsPages.workspaces.toast.removeMemberFailed'),
				variant: 'destructive',
			});
		} finally {
			removingMemberId.value = null;
			memberToRemove.value = null;
		}
	}

	watch(
		() => activeWorkspace.value?.value,
		() => {
			members.value = [];
			membersLoaded.value = false;
			if (activeWorkspace.value && !isLocalActiveWorkspace.value) {
				loadMembers();
			}
		},
		{ immediate: true },
	);
</script>

<template>
	<PageContainer width="narrow">
		<PageHeader :title="$t('settingsPages.workspaces.title')">
			<template #actions>
				<div class="flex flex-wrap items-center gap-2">
					<Dialog v-model:open="isOpen">
						<DialogTrigger as-child>
							<Button variant="default">
								<FolderKanbanIcon />
								{{ $t('settingsPages.workspaces.newWorkspace') }}
							</Button>
						</DialogTrigger>

						<DialogContent class="sm:max-w-[425px]">
							<DialogHeader>
								<DialogTitle>{{
									$t('settingsPages.workspaces.create.title')
								}}</DialogTitle>
								<DialogDescription class="sr-only">{{
									$t('settingsPages.workspaces.create.description')
								}}</DialogDescription>
							</DialogHeader>

							<Input
								v-model="newWorkspace.name"
								:placeholder="
									$t('settingsPages.workspaces.create.namePlaceholder')
								"
							/>

							<DialogFooter>
								<Button
									variant="default"
									type="submit"
									:disabled="newWorkspace.name.trim() === ''"
									@click="createWorkspace"
								>
									{{ $t('common.actions.create') }}
								</Button>
							</DialogFooter>
						</DialogContent>
					</Dialog>
					<Dialog
						v-if="!isLocalActiveWorkspace"
						v-model:open="isOpenInvitation"
					>
						<DialogTrigger as-child>
							<Button
								variant="outline"
								:aria-label="$t('settingsPages.workspaces.invite.label')"
								data-tour="workspace.members"
							>
								<UserPlus />
								<span class="hidden lg:inline">{{
									$t('settingsPages.workspaces.invite.label')
								}}</span>
							</Button>
						</DialogTrigger>

						<DialogContent class="sm:max-w-[425px]">
							<DialogHeader>
								<DialogTitle>{{
									$t('settingsPages.workspaces.invite.title')
								}}</DialogTitle>
								<DialogDescription class="sr-only">{{
									$t('settingsPages.workspaces.invite.description')
								}}</DialogDescription>
							</DialogHeader>

							<Textarea
								v-model="invitationEmails"
								rows="5"
								:placeholder="
									$t('settingsPages.workspaces.invite.emailsPlaceholder')
								"
							/>
							<span
								class="text-xs whitespace-pre text-destructive"
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
										invitationEmails.trim() === ''
									"
									@click="sendInvitations"
								>
									{{ $t('settingsPages.workspaces.invite.send') }}
								</Button>
							</DialogFooter>
						</DialogContent>
					</Dialog>
					<template
						v-if="
							!isLoading &&
							!isLocalActiveWorkspace &&
							activeWorkspaceType !== 'default' &&
							isOwnedActiveWorkspace
						"
					>
						<Button
							variant="outline"
							:aria-label="$t('settingsPages.workspaces.delete.label')"
							class="text-destructive hover:text-destructive"
							@click="isDeleteDialogOpen = true"
						>
							<Trash2Icon />
							<span class="hidden lg:inline">{{
								$t('settingsPages.workspaces.delete.label')
							}}</span>
						</Button>
						<ConfirmByNameDialog
							v-model:open="isDeleteDialogOpen"
							:title="$t('settingsPages.workspaces.delete.label')"
							:description="
								$t('settingsPages.workspaces.delete.description', {
									name: activeWorkspaceName,
								})
							"
							:expected-name="activeWorkspaceName"
							:confirm-label="$t('settingsPages.workspaces.delete.label')"
							:busy="isDeleting"
							@confirm="deleteWorkspace"
						/>
					</template>
					<AlertDialog
						v-if="
							!isLoading && !isLocalActiveWorkspace && !isOwnedActiveWorkspace
						"
					>
						<AlertDialogTrigger as-child>
							<Button
								variant="outline"
								class="text-destructive hover:text-destructive"
							>
								<LogOutIcon />
								<span class="hidden lg:inline">{{
									$t('settingsPages.workspaces.exit.label')
								}}</span>
							</Button>
						</AlertDialogTrigger>
						<AlertDialogContent>
							<AlertDialogHeader>
								<AlertDialogTitle>{{
									$t('settingsPages.workspaces.exit.confirmTitle')
								}}</AlertDialogTitle>
								<AlertDialogDescription>
									{{ $t('settingsPages.workspaces.exit.confirmDescription') }}
								</AlertDialogDescription>
							</AlertDialogHeader>
							<AlertDialogFooter>
								<AlertDialogCancel>{{
									$t('common.actions.cancel')
								}}</AlertDialogCancel>
								<AlertDialogAction
									@click="exitWorkspace"
									class="bg-destructive text-destructive-foreground shadow-xs hover:bg-destructive/90"
								>
									{{ $t('settingsPages.workspaces.exit.label') }}
								</AlertDialogAction>
							</AlertDialogFooter>
						</AlertDialogContent>
					</AlertDialog>
				</div>
			</template>
		</PageHeader>
		<AsyncContent
			:pending="isLoading"
			:loaded="initialLoaded"
			:error="initialError"
			:retry="loadWorkspaceSettings"
			:label="$t('settingsPages.workspaces.loading')"
		>
			<div class="flex flex-col gap-6">
				<SettingsSection
					v-if="!isLocalActiveWorkspace && activeWorkspaceItem"
					:title="$t('settingsPages.workspaces.general.title')"
					data-testid="workspace-general"
				>
					<form
						v-if="isOwnedActiveWorkspace"
						id="workspace-general-form"
						class="flex flex-col gap-4"
						@submit.prevent="saveWorkspace"
					>
						<div class="flex flex-col gap-1.5">
							<Label for="workspace-name">{{
								$t('settingsPages.workspaces.general.name')
							}}</Label>
							<Input
								id="workspace-name"
								v-model="workspaceName"
								:disabled="savingWorkspace"
								maxlength="255"
								autocomplete="off"
								:aria-invalid="trimmedWorkspaceName === ''"
								aria-describedby="workspace-name-hint"
							/>
							<p
								v-if="trimmedWorkspaceName === ''"
								id="workspace-name-hint"
								class="text-xs text-status-fix-fg"
								data-testid="workspace-name-hint"
							>
								{{ $t('settingsPages.workspaces.general.nameEmpty') }}
							</p>
						</div>

						<div class="flex flex-col gap-1.5">
							<span id="workspace-color-label" class="text-sm font-medium">
								{{ $t('settingsPages.workspaces.general.colour') }}
							</span>
							<div
								role="radiogroup"
								aria-labelledby="workspace-color-label"
								class="flex flex-wrap items-center gap-2.5 p-1"
							>
								<button
									type="button"
									role="radio"
									:aria-label="
										$t('settingsPages.workspaces.general.defaultColour')
									"
									:title="$t('settingsPages.workspaces.general.defaultColour')"
									:aria-checked="workspaceColor === ''"
									data-testid="workspace-color-default"
									:class="[
										'flex size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground transition',
										workspaceColor === '' ? 'ring-2 ring-ink' : '',
									]"
									:disabled="savingWorkspace"
									@click="workspaceColor = ''"
								>
									<PackageOpen class="size-4" />
								</button>
								<button
									v-for="swatch in WORKSPACE_COLORS"
									:key="swatch.value"
									type="button"
									role="radio"
									:aria-label="swatch.name"
									:title="swatch.name"
									:aria-checked="workspaceColor === swatch.value"
									:data-testid="`workspace-color-${swatch.value.slice(1)}`"
									:class="[
										'flex size-8 items-center justify-center rounded-lg transition',
										workspaceColor === swatch.value ? 'ring-2 ring-ink' : '',
									]"
									:style="workspaceIconStyle(swatch.value)"
									:disabled="savingWorkspace"
									@click="workspaceColor = swatch.value"
								>
									<CheckIcon
										v-if="workspaceColor === swatch.value"
										class="size-4"
									/>
								</button>
							</div>
						</div>
					</form>
					<SettingsRow
						v-else
						:label="$t('settingsPages.workspaces.general.name')"
					>
						<span
							class="text-sm text-ink"
							data-testid="workspace-name-readonly"
						>
							{{ activeWorkspaceItem.name }}
						</span>
					</SettingsRow>

					<template v-if="isOwnedActiveWorkspace" #footer>
						<Button
							type="submit"
							form="workspace-general-form"
							variant="default"
							data-testid="workspace-save"
							:disabled="
								savingWorkspace ||
								trimmedWorkspaceName === '' ||
								!workspaceDirty
							"
							:aria-busy="savingWorkspace"
						>
							<SaveIcon /> {{ $t('common.actions.save') }}
						</Button>
					</template>
				</SettingsSection>

				<SettingsSection
					:title="$t('settingsPages.workspaces.preferences.title')"
				>
					<div class="flex flex-col gap-4">
						<div
							v-for="(setting, index) in settings"
							:key="setting.id"
							class="flex flex-col gap-1.5"
						>
							<Label :for="`setting-${setting.id}`">{{ setting.name }}</Label>

							<template v-if="setting.component_type === 'select'">
								<Combobox
									:entities="setting.default_values"
									v-model="settings[index].value"
									:selected-placeholder="setting.description"
									value-key="value"
									label-key="value"
								/>
							</template>
							<template v-else-if="setting.custom_value_available">
								<Input
									:id="`setting-${setting.id}`"
									type="time"
									v-if="setting.component_type === 'time_in_seconds'"
									:model-value="convertToHHMM(settings[index].value)"
									@input="
										(e: InputEvent) =>
											(settings[index].value = timeToSeconds(
												(e.target as HTMLInputElement).value,
											))
									"
									:placeholder="setting.description"
								/>

								<Input
									:id="`setting-${setting.id}`"
									v-else
									v-model="settings[index].value"
									:placeholder="setting.description"
								/>
							</template>

							<SettingsRow
								v-if="
									setting.custom_value_available &&
									setting.default_values &&
									setting.default_values.length > 0
								"
								v-slot="{ labelId }"
								:label="$t('settingsPages.workspaces.preferences.customValue')"
							>
								<Switch
									:checked="setting.show_custom_value_input"
									@update:checked="(v) => (setting.show_custom_value_input = v)"
									:aria-labelledby="labelId"
								/>
							</SettingsRow>
						</div>
					</div>

					<template #footer>
						<Button
							variant="default"
							@click="updateSettings"
							:disabled="savingSettings"
							:aria-busy="savingSettings"
						>
							<SaveIcon /> {{ $t('common.actions.save') }}
						</Button>
					</template>
				</SettingsSection>

				<SettingsSection :title="$t('settingsPages.workspaces.default.title')">
					<SettingsRow
						:label="$t('settingsPages.workspaces.default.title')"
						:description="$t('settingsPages.workspaces.default.description')"
					>
						<Combobox
							:entities="cloudWorkspaceOptions"
							v-model="defaultWorkspaceSelection"
							:selected-placeholder="
								$t('settingsPages.workspaces.default.choose')
							"
							value-key="id"
							label-key="name"
						/>
					</SettingsRow>

					<template #footer>
						<Button
							variant="default"
							@click="saveDefaultWorkspace"
							:disabled="
								savingDefaultWorkspace || defaultWorkspaceSelection == null
							"
							:aria-busy="savingDefaultWorkspace"
						>
							<SaveIcon />
							{{ $t('settingsPages.workspaces.default.setAsDefault') }}
						</Button>
					</template>
				</SettingsSection>

				<SettingsSection
					v-if="!isLocalActiveWorkspace"
					:title="$t('settingsPages.workspaces.members.title')"
				>
					<AsyncContent
						:pending="loadingMembers"
						:loaded="membersLoaded"
						:error="membersError"
						:retry="loadMembers"
						:label="$t('settingsPages.workspaces.members.loading')"
					>
						<p v-if="members.length === 0" class="text-sm text-ink-subtle">
							{{ $t('settingsPages.workspaces.members.none') }}
						</p>

						<div v-else class="space-y-3">
							<div
								v-for="member in members"
								:key="member.id"
								class="flex items-center justify-between rounded-lg border border-border p-3"
							>
								<div class="flex flex-1 items-center gap-3">
									<Avatar class="h-10 w-10">
										<AvatarFallback>
											{{ getInitials(member.name) }}
										</AvatarFallback>
									</Avatar>

									<div class="flex-1">
										<div class="flex items-center gap-2">
											<p class="font-medium text-ink">{{ member.name }}</p>
											<span
												v-if="
													member.id ===
													workspaces.find((w) => w.id == activeWorkspace?.value)
														?.user_id
												"
												class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold text-muted-foreground uppercase"
											>
												{{ $t('settingsPages.workspaces.members.owner') }}
											</span>
										</div>
									</div>
								</div>

								<Button
									v-if="canRemoveMember(member)"
									@click="handleRemoveMember(member)"
									variant="destructive"
									size="sm"
									:disabled="removingMemberId !== null"
								>
									<Loader
										v-if="removingMemberId === member.id"
										is-mini
										class="mr-2"
									/>
									{{ $t('common.actions.remove') }}
								</Button>
							</div>
						</div>
					</AsyncContent>
				</SettingsSection>

				<SettingsSection
					v-if="!isLocalActiveWorkspace"
					:title="$t('settingsPages.workspaces.invitations.title')"
				>
					<WorkspaceInvitationsList />
				</SettingsSection>
			</div>

			<AlertDialog v-model:open="showRemoveMemberDialog">
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle>{{
							$t('settingsPages.workspaces.members.removeTitle')
						}}</AlertDialogTitle>
						<AlertDialogDescription>
							<i18n-t
								keypath="settingsPages.workspaces.members.removeConfirm"
								tag="span"
							>
								<template #name>
									<strong>{{ memberToRemove?.name }}</strong>
								</template>
							</i18n-t>
						</AlertDialogDescription>
					</AlertDialogHeader>
					<AlertDialogFooter>
						<AlertDialogCancel @click="showRemoveMemberDialog = false">{{
							$t('common.actions.cancel')
						}}</AlertDialogCancel>
						<AlertDialogAction
							@click="confirmRemoveMember"
							class="bg-destructive text-destructive-foreground shadow-xs hover:bg-destructive/90"
						>
							{{ $t('common.actions.remove') }}
						</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>
		</AsyncContent>
	</PageContainer>
</template>
