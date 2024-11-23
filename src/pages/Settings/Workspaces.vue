<script setup lang="ts">
	import { SaveIcon } from 'lucide-vue-next';
	import { Button } from '@/components/ui/button';
	import Switcher from '@/components/general/Switcher.vue';
	import { onBeforeMount, Ref, ref } from 'vue';
	import { getUserSettingsV2, updateUserSettingsV2 } from '@/actions/tmgr/user';
	import { Input } from '@/components/ui/input';
	import store from '@/store';
	import Combobox from '@/components/Combobox.vue';
	import { getWorkspaces, Workspace } from '@/actions/tmgr/workspaces.js';
	import { convertToHHMM, timeToSeconds } from '@/utils/timeUtils';

	const settings = ref();
	const workspaces: Ref<Workspace[]> = ref([]);

	onBeforeMount(async () => {
		workspaces.value = await getWorkspaces();
		const loadedSettings = await getUserSettingsV2();
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

		settings.value = [
			...mappedSettingWithUserSettings.filter(
				(setting) => setting.key === 'current_workspace',
			),
			...mappedSettingWithUserSettings.filter(
				(setting) => setting.key !== 'current_workspace',
			),
		];
	});

	async function updateSettings() {
		try {
			await Promise.all([updateUserSettingsV2(settings.value)]);
		} catch (e) {
			console.error(e);
		}
	}
</script>

<template>
	<teleport to="title">Workspace settings</teleport>

	<div class="flex max-w-lg flex-col gap-3 p-4">
		<h3 class="mb-4 text-lg font-bold">Workspace Settings</h3>
		<div>
			<div v-for="(setting, index) in settings">
				<label
					:for="`setting-${setting.id}`"
					class="mb-2 block text-sm font-bold text-gray-700"
				>
					{{ setting.name }}
				</label>

				<div class="relative mb-4">
					<template v-if="setting.component_type === 'current_workspace'">
						<!--						<CurrentWorkspace v-model="settings[index].value" />-->
						<Combobox
							:entities="workspaces"
							v-model="settings[index].value"
							selected-placeholder="Workspace"
							value-key="id"
							label-key="name"
							input-placeholder="Search workspaces"
						/>
					</template>
					<template v-else-if="setting.component_type === 'select'">
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

		<div class="text-left">
			<Button variant="default" @click="updateSettings">
				<SaveIcon /> Save
			</Button>
		</div>
	</div>
</template>
