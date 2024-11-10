<template>
	<Dialog>
		<DialogTrigger as-child>
			<button type="button" title="Settings">
				<CogIcon
					class="size-6 fill-gray-400 transition hover:fill-black dark:hover:fill-white"
				/>
			</button>
		</DialogTrigger>

		<DialogContent
			class="!rounded-[8px] bg-white dark:border-transparent dark:bg-gray-900 dark:text-white sm:max-w-[425px]"
		>
			<DialogHeader>
				<DialogTitle>Settings</DialogTitle>
				<DialogDescription>
					Make changes to the task settings here. Click save when you're done.
				</DialogDescription>
			</DialogHeader>

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
						<template v-else-if="setting.component_type === 'select'">
							<Select
								v-model="settings.value"
								:options="setting.default_values.map(val => ({
												label: val.value,
												value: val.value
											}))"
								:placeholder="setting.description"
							/>
						</template>
						<TextField
							v-else
							v-model="setting.value"
							:placeholder="setting.description"
						/>
					</div>
				</div>
			</div>

			<DialogFooter>
				<button
					type="submit"
					class="w-full rounded bg-tmgr-light-blue p-2 text-white hover:opacity-90"
				>
					Save
				</button>
			</DialogFooter>
		</DialogContent>
	</Dialog>
	<!--	<form
		v-if="settings.length > 0"
		@submit.prevent="saveSettings"
		class="text-gray-800 dark:text-tmgr-gray"
	>
		<button @click="emit('close')" class="absolute right-6 top-7">
			<XMarkIcon
				class="size-5 fill-neutral-600 hover:fill-black dark:hover:fill-white"
			/>
		</button>

		<button
			type="submit"
			class="w-full rounded bg-tmgr-light-blue p-2 text-white"
		>
			Update
		</button>
	</form>-->
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
	import { CogIcon, XMarkIcon } from '@heroicons/vue/20/solid';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from 'src/components/ui/dialog';
	import { Button } from 'src/components/ui/button';
	import Select from 'src/components/general/Select.vue';

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
