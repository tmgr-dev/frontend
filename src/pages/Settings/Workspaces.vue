<script setup lang="ts">
	import {
		SaveIcon,
		FolderKanbanIcon,
		Trash2Icon,
		UserPlus,
		CircleCheckBigIcon,
		LogOutIcon,
	} from 'lucide-vue-next';
	import { Button } from '@/components/ui/button';
	import Switcher from '@/components/general/Switcher.vue';
	import { computed, onBeforeMount, Ref, ref, watch } from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import { getUserSettings, getUserSettingsV2, updateUserSettingsV2 } from '@/actions/tmgr/user';
	import { Input } from '@/components/ui/input';
	import store from '@/store';
	import Combobox from '@/components/Combobox.vue';
	import {
		createWorkspace as createWorkspaceAction,
		createWorkspaceInvitation as createWorkspaceInvitationAction,
		deleteWorkspace as deleteWorkspaceAction,
		exitWorkspace as exitWorkspaceAction,
		getWorkspaces,
		Workspace,
	} from '@/actions/tmgr/workspaces';
	import { convertToHHMM, timeToSeconds } from '@/utils/timeUtils';
	import { FormSetting } from '@/actions/tmgr/settings.ts';
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
	import { useToast } from '@/components/ui/toast';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import { dialogState } from '@/composable/dialog.ts';
	import { Textarea } from '@/components/ui/textarea';
	import { validateEmailString, ValidationResult } from '@/utils/emails.ts';

	const toaster = useToast();
	const [isOpen, closeDialog] = dialogState();
	const [isOpenInvitation, closeInvitationDialog] = dialogState();
	const newWorkspace = ref({
		name: '',
		type: 'test',
	});
	const invitationEmails = ref('');
	const invitationEmailsValidationError = ref<ValidationResult>({
		isValid: true,
		errors: []
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
	const isOwnedActiveWorkspace = computed(() => {
		const foundWorkspace = workspaces.value.find(
			(workspace) => workspace.id == activeWorkspace.value?.value,
		);

		return foundWorkspace?.user_id === store.state.user.id;
	});
	const isLoading = ref(true);

	function openCreateWorkspaceModalIfQueryParamHasCreateKey (query: any) {
		if (query.hasOwnProperty('create')) {
			router.push(route.path);
		}
	}

	watch(isOpen, (newValue) => {
		if (!newValue) {
			openCreateWorkspaceModalIfQueryParamHasCreateKey(route.query);
		}
	});

	watch(
		route,
		(to) => {
			if (!isOpen.value && to.query.hasOwnProperty('create')) {
				isOpen.value = true;
			}
		},
		{
			flush: 'pre',
			immediate: true,
			deep: true
		}
	);

	watch(invitationEmails, newValue => {
		invitationEmailsValidationError.value = validateEmailString(newValue);
	})

	onBeforeMount(async () => {
		const [loadedWorkspaces, loadedSettings] = await Promise.all([
			getWorkspaces(),
			getUserSettingsV2(),
		]);
		isOpen.value = route.query.hasOwnProperty('create');
		isLoading.value = false;
		workspaces.value = loadedWorkspaces;
		console.log(store.state.user.settings);
		activeWorkspace.value = store.state.user.settings.find(
			(settingInStore: any) => settingInStore.key === 'current_workspace',
		);

		const mappedSettingWithUserSettings = loadedSettings.map((setting) => {
			let settingFromStoreWithValue = store.state.user.settings.find(
				(settingInStore: any) => settingInStore.id === setting.id,
			);
			settingFromStoreWithValue = settingFromStoreWithValue
				? {
						...setting,
						...settingFromStoreWithValue,
				  }
				: {
						...setting,
						value: null,
				  };

			return settingFromStoreWithValue;
		});

		settings.value = mappedSettingWithUserSettings.filter(
			(setting) => setting.key !== 'current_workspace',
		);
	});

	async function updateSettings() {
		try {
			const updatedSettings = [...settings.value, activeWorkspace.value];
			
			// Store original editor value to check if it changes
			const preferredEditorSetting = settings.value.find((setting: any) => setting.key === 'preferred_editor');
			const originalEditorValue = preferredEditorSetting?.value;
			
			// Always update localStorage if preferred_editor setting exists
			// This ensures the value is consistently saved
			if (preferredEditorSetting) {
				// Make sure we convert to string and handle null/undefined values
				let editorValue = preferredEditorSetting.value 
					? String(preferredEditorSetting.value).toLowerCase().trim() 
					: '';
					
				// Validate the editor value
				if (editorValue && (editorValue === 'block' || editorValue === 'markdown')) {
					console.log('Saving editor preference to localStorage:', editorValue);
					localStorage.setItem('preferred_editor', editorValue);
					
					// Also update the editor value in the Vuex store
					if (store.state.user && store.state.user.settings) {
						// Update the store's user settings directly 
						store.state.user.settings = store.state.user.settings.map((setting: any) => {
							if (setting.key === 'preferred_editor') {
								return { ...setting, value: editorValue };
							}
							return setting;
						});
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
				
				if (normalizedNewValue && normalizedNewValue !== normalizedOriginalValue) {
					toaster.toast({
						variant: 'default',
						title: 'Editor updated. Reloading page...',
						action: CircleCheckBigIcon,
						class: 'bg-green-500 border-0 text-white',
					});
					
					// Force localStorage update one more time to ensure it's saved before reload
					if (normalizedNewValue === 'block' || normalizedNewValue === 'markdown') {
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
				title: 'Successfully saved!',
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});
		} catch (e) {
			console.error(e);
		}
	}

	async function createWorkspace() {
		if (newWorkspace.value.name.trim() === '') return;

		try {
			await createWorkspaceAction(newWorkspace.value);
			newWorkspace.value.name = '';
			closeDialog();
			toaster.toast({
				title: 'Successfully created!',
				action: CircleCheckBigIcon,
				class: 'bg-green-500 border-0 text-white',
			});

			setTimeout(() => {
				window.location.reload();
			}, 100);
		} catch (e) {
			console.error(e);
		}
	}

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

		if (!activeWorkspace?.value?.value) {
			return;
		}

		const workspaceId = activeWorkspace.value.value;
		await createWorkspaceInvitationAction(
			typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId, 
			{ emails }
		);
		isOpenInvitation.value = false;
		invitationEmails.value = '';
	}

	async function updateUserSettings () {
		await getUserSettings();
		setTimeout(() => {
			window.location.reload();
		}, 100);
	}

	async function deleteWorkspace() {
		try {
			const workspaceId = activeWorkspace.value?.value;
			if (workspaceId) {
				await deleteWorkspaceAction(typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId);
				await updateUserSettings();
			}
		} catch (e) {
			console.error(e);
		}
	}

	async function exitWorkspace() {
		try {
			const workspaceId = activeWorkspace.value?.value;
			if (workspaceId) {
				await exitWorkspaceAction(typeof workspaceId === 'string' ? parseInt(workspaceId) : workspaceId);
				await updateUserSettings();
			}
		} catch (e) {
			console.error(e);
		}
	}
</script>

<template>
	<teleport to="title">Workspace settings</teleport>

	<div class="container">
		<header class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
			<h3 class="text-lg font-bold">Workspace Settings</h3>

			<div class="flex flex-wrap items-center gap-2">
				<Dialog v-model:open="isOpen">
					<DialogTrigger as-child>
						<Button variant="default">
							<FolderKanbanIcon />
							New workspace
						</Button>
					</DialogTrigger>

					<DialogContent
						class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
					>
						<DialogHeader>
							<DialogTitle>Creating new workspace</DialogTitle>
						</DialogHeader>

						<Input
							v-model="newWorkspace.name"
							placeholder="New workspace name"
						/>

						<DialogFooter>
							<Button
								variant="default"
								type="submit"
								:disabled="newWorkspace.name.trim() === ''"
								@click="createWorkspace"
							>
								Create
							</Button>
						</DialogFooter>
					</DialogContent>
				</Dialog>
				<Dialog v-model:open="isOpenInvitation">
					<DialogTrigger as-child>
						<Button variant="default">
							<UserPlus />
							<span class="hidden lg:inline">Invite</span>
						</Button>
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
								:disabled="!invitationEmailsValidationError.isValid || invitationEmails.trim() === ''"
								@click="sendInvitations"
							>
								Send
							</Button>
						</DialogFooter>
					</DialogContent>
				</Dialog>
				<AlertDialog v-if="!isLoading && activeWorkspaceType !== 'default' && isOwnedActiveWorkspace">
					<AlertDialogTrigger as-child>
						<Button variant="destructive">
							<Trash2Icon />
							<span class="hidden lg:inline">Delete workspace</span>
						</Button>
					</AlertDialogTrigger>
					<AlertDialogContent>
						<AlertDialogHeader>
							<AlertDialogTitle>Are you absolutely sure?</AlertDialogTitle>
							<AlertDialogDescription>
								This action cannot be undone. This will delete your
								workspace.
							</AlertDialogDescription>
						</AlertDialogHeader>
						<AlertDialogFooter>
							<AlertDialogCancel>Cancel</AlertDialogCancel>
							<AlertDialogAction
								@click="deleteWorkspace"
								class="bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90"
							>
								Delete workspace
							</AlertDialogAction>
						</AlertDialogFooter>
					</AlertDialogContent>
				</AlertDialog>
				<AlertDialog v-if="!isLoading && !isOwnedActiveWorkspace">
					<AlertDialogTrigger as-child>
						<Button variant="destructive">
							<LogOutIcon />
							<span class="hidden lg:inline">Exit from workspace</span>
						</Button>
					</AlertDialogTrigger>
					<AlertDialogContent>
						<AlertDialogHeader>
							<AlertDialogTitle>Are you absolutely sure?</AlertDialogTitle>
							<AlertDialogDescription>
								This action cannot be undone by yourself. If you wanna come back to this workspace ask a member of this workspace to send you an invitation again.
							</AlertDialogDescription>
						</AlertDialogHeader>
						<AlertDialogFooter>
							<AlertDialogCancel>Cancel</AlertDialogCancel>
							<AlertDialogAction
								@click="exitWorkspace"
								class="bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90"
							>
								Exit from workspace
							</AlertDialogAction>
						</AlertDialogFooter>
					</AlertDialogContent>
				</AlertDialog>
			</div>
		</header>

		<div class="mt-6 max-w-lg">
			<div v-for="(setting, index) in settings">
				<label
					:for="`setting-${setting.id}`"
					class="mb-2 block text-sm font-bold text-gray-700"
				>
					{{ setting.name }}
				</label>

				<div class="mb-4">
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
							type="time"
							v-if="setting.component_type === 'time_in_seconds'"
							:model-value="convertToHHMM(settings[index].value)"
							@input="
								(e: InputEvent) => (settings[index].value = timeToSeconds((e.target as HTMLInputElement).value))
							"
							:placeholder="setting.description"
						/>

						<Input
							v-else
							v-model="settings[index].value"
							:placeholder="setting.description"
						/>
					</template>

					<Switcher
						v-if="
							setting.custom_value_available &&
							setting.default_values &&
							setting.default_values.length > 0
						"
						name="set_custom_value"
						v-model="setting.show_custom_value_input"
						placeholder="Set custom value"
					/>
				</div>
			</div>
		</div>

		<footer class="text-left">
			<Button variant="default" @click="updateSettings">
				<SaveIcon /> Save
			</Button>
		</footer>
	</div>
</template>
