<template>
	<form
		@submit.prevent="updateCategory"
		class="text-gray-800 dark:text-tmgr-gray"
	>
		<label for="settings" class="mb-5 block text-lg font-bold">
			Settings
		</label>

		<div>
			<div
				v-if="form.user"
				class="estimated-info border-b border-neutral-200 py-1 pr-5 text-start dark:border-neutral-600"
			>
				Author:
				<span class="text-neutral-600 dark:text-neutral-300">
					{{ form.user.name }}
				</span>
			</div>

			<div
				v-for="(setting, index) in availableSettings"
				id="settings"
				class="mt-4"
			>
				<label
					:for="`setting-${setting.id}`"
					class="mb-2 block text-left text-sm font-bold"
				>
					{{ setting.name }}
				</label>

				<div class="relative mb-4">
					<TimeField
						v-if="setting.component_type === 'time_in_seconds'"
						v-model="settings[index].value"
						:placeholder="setting.description"
					/>

					<TextField
						v-else
						v-model="settings[index].value"
						:placeholder="setting.description"
					/>
				</div>
			</div>

			<div class="estimated-info py-2 pr-5 text-start">
				<div
					class="flex items-center border-b border-neutral-200 dark:border-neutral-600"
				>
					Assignees
					<span
						class="material-icons checkpoint-delete text-lg"
						@click="(isShowModalAssign = true), $store.commit('openModal')"
					>
						add
					</span>
				</div>

				<div
					v-if="form.assignees && form.assignees.length"
					v-for="assignee in form.assignees"
					class="mt-2 flex gap-2"
				>
					<div class="flex items-center gap-1.5">
						{{ assignee.name }}

						<span
							class="material-icons checkpoint-delete text-lg text-red-500"
							@click="deleteAssignee(assignee.id)"
						>
							person_remove
						</span>
					</div>
				</div>
			</div>
		</div>

		<div class="mt-6 flex flex-nowrap items-center">
			<button
				type="button"
				@click="closingModal"
				class="mr-1 block w-2/4 rounded bg-gray-700 p-2 text-white"
			>
				Cancel
			</button>

			<button
				type="submit"
				class="mr-1 block w-2/4 rounded bg-blue-700 p-2 text-white"
			>
				Update
			</button>
		</div>
	</form>
</template>

<script setup lang="ts">
	import TimeField from 'src/components/general/TimeField.vue';
	import TextField from 'src/components/general/TextField.vue';
	import {
		getTaskSettings,
		updateOneTaskSettings,
	} from 'src/actions/tmgr/settings';
	import { ref } from 'vue';

	interface Props {
		form: Record<string, unknown>; // @todo describe form type
	}

	const props = defineProps<Props>();
	const availableSettings = ref<Record<string, any>[]>([]);

	async function updateCategory() {
		if (this.currentCategoryOptionInSelect) {
			this.form.project_category_id = this.currentCategoryOptionInSelect;
		}
		this.isShowSettingsModal = false;
		this.$store.commit('closeModal');
		this.isShowAlert = true;
		await this.saveTask();
		await this.loadCategory();
	}

	async function loadTaskSettings() {
		const data = await getTaskSettings();
		this.initSettings(data, this.form.settings);
		this.availableSettings = data;
	}

	function initSettings(availableSettings, settings = []) {
		return availableSettings.map((item, index) => {
			const setting = this.getSettingById(settings, item.id, {
				id: item.id,
				value: '',
			});
			this.settings[index] = setting;
			item.show_custom_value_input =
				item.default_values &&
				item.default_values.findIndex((val) => val.value === setting.value) ===
					-1;
		});
	}

	async function saveSettings(settings) {
		const data = await updateOneTaskSettings(this.form.id, settings);
		this.initSettings(this.availableSettings, data.settings);
	}
</script>
