<script setup lang="ts">
	import {
		SaveIcon,
		FolderKanbanIcon,
		Trash2Icon,
		CircleCheckBigIcon,
	} from 'lucide-vue-next';
	import { Button } from '@/components/ui/button';
	import Switcher from '@/components/general/Switcher.vue';
	import { computed, onBeforeMount, Ref, ref } from 'vue';
	import { getUserSettingsV2, updateUserSettingsV2 } from '@/actions/tmgr/user';
	import { Input } from '@/components/ui/input';
	import store from '@/store';
	import Combobox from '@/components/Combobox.vue';
	import {
		createWorkspace as createWorkspaceAction,
		deleteWorkspace as deleteWorkspaceAction,
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

	const toaster = useToast();
	const [isOpen, closeDialog] = dialogState();
	const newWorkspace = ref({
		name: '',
		type: 'test',
	});

	const settings = ref();
	const workspaces: Ref<Workspace[]> = ref([]);
	const activeWorkspace = ref<FormSetting>();
	const activeWorkspaceType = computed(() => {
		const foundWorkspace = workspaces.value.find(
			(workspace) => workspace.id == activeWorkspace.value?.value,
		);

		return foundWorkspace?.type;
	});
	const isLoading = ref(true);

	onBeforeMount(async () => {
		const [loadedWorkspaces, loadedSettings] = await Promise.all([
			getWorkspaces(),
			getUserSettingsV2(),
		]);
		isLoading.value = false;
		workspaces.value = loadedWorkspaces;
		activeWorkspace.value = store.state.user.settings.find(
			(settingInStore) => settingInStore.key === 'current_workspace',
		);

		const mappedSettingWithUserSettings = loadedSettings.map((setting) => {
			let settingFromStoreWithValue = store.state.user.settings.find(
				(settingInStore) => settingInStore.id === setting.id,
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

			await updateUserSettingsV2(updatedSettings);
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

	async function deleteWorkspace() {
		try {
			await deleteWorkspaceAction(activeWorkspace.value!.id);
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
				<AlertDialog v-if="!isLoading && activeWorkspaceType !== 'default'">
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
								This action cannot be undone. This will permanently delete your
								workspace and remove your data from our servers.
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
