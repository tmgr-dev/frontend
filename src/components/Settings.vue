<template>
	<form
		@submit.prevent="saveSettings"
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

			<div v-for="(setting, index) in settings" id="settings" class="mt-4">
				<label
					:for="`setting-${setting.id}`"
					class="mb-2 block text-left text-sm font-bold"
				>
					{{ setting.name }}
				</label>

				<div class="relative mb-4">
					<TimeField
						v-if="setting.component_type === 'time_in_seconds'"
						v-model="setting.value"
						:placeholder="setting.description"
					/>

					<TextField
						v-else
						v-model="setting.value"
						:placeholder="setting.description"
					/>
				</div>
			</div>
		</div>

		<div class="mt-6 flex flex-nowrap items-center">
			<button
				type="button"
				@click="emit('close')"
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
		FormSetting,
		getTaskSettings,
		Setting,
		updateOneTaskSettings,
	} from 'src/actions/tmgr/settings';
	import { onBeforeMount, ref } from 'vue';
	import { Task } from 'src/actions/tmgr/tasks';

	const emit = defineEmits(['close']);

	interface Props {
		form: Task;
	}

	const props = defineProps<Props>();
	const settings = ref<Setting[]>([]);

	onBeforeMount(async () => {
		await loadTaskSettings();
	});

	async function loadTaskSettings() {
		settings.value = await getTaskSettings();
	}

	async function saveSettings(settings: FormSetting) {
		try {
			await updateOneTaskSettings(props.form.id, settings);
		} catch (e) {
			console.error(e);
			throw e;
		}
	}
</script>
