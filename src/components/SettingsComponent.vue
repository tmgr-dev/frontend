<template>
	<Dialog>
		<DialogTrigger as-child>
			<button
				type="button"
				title="Settings"
				class="flex h-7 w-7 items-center justify-center rounded-pill text-ink-subtle hover:bg-surface-hover hover:text-ink"
			>
				<CogIcon class="size-4" />
			</button>
		</DialogTrigger>

		<DialogContent
			class="rounded-card border border-line bg-surface text-ink sm:max-w-[425px]"
		>
			<DialogHeader>
				<DialogTitle class="text-ink">Settings</DialogTitle>
				<DialogDescription class="text-ink-muted">
					Make changes to the task settings here. Click save when you're done.
				</DialogDescription>
			</DialogHeader>

			<div>
				<div v-for="setting in settings" :key="setting.id" id="settings" class="mt-4">
					<label
						:for="`setting-${setting.id}`"
						class="mb-2 block text-left text-2xs font-bold uppercase tracking-wide text-ink-subtle"
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
								v-model="setting.value"
								:options="
									setting.default_values.map((val) => ({
										label: val.value,
										value: val.value,
									}))
								"
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
				<DialogClose as-child>
					<button
						type="button"
						class="w-full rounded-md bg-brand px-3 py-2 text-sm font-semibold text-white hover:bg-brand-hover"
						@click="handleSave"
					>
						Save
					</button>
				</DialogClose>
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
	import TimeField from '@/components/general/TimeField.vue';
	import TextField from '@/components/general/TextField.vue';
	import {
		getTaskSettings,
		Setting,
		SettingPayload,
		updateTaskSettings,
	} from '@/actions/tmgr/settings';
	import { onBeforeMount, ref, watch } from 'vue';
	import { Task } from '@/actions/tmgr/tasks';
	import { CogIcon } from '@heroicons/vue/20/solid';
	import {
		Dialog,
		DialogClose,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
		DialogTrigger,
	} from '@/components/ui/dialog';
	import Select from '@/components/general/Select.vue';

	const emit = defineEmits(['close']);

	interface SettingWithValue extends Setting {
		value: any;
	}

	interface Props {
		form: Task;
	}

	const props = defineProps<Props>();
	const settings = ref<SettingWithValue[]>([]);
	const settingsSchema = ref<Setting[]>([]);

	onBeforeMount(async () => {
		settingsSchema.value = await getTaskSettings();
		mergeSettingsWithFormValues();
	});

	watch(
		() => props.form.settings,
		() => {
			mergeSettingsWithFormValues();
		},
		{ deep: true }
	);

	function mergeSettingsWithFormValues() {
		if (!settingsSchema.value.length) return;
		
		settings.value = settingsSchema.value.map((setting: Setting) => {
			const taskSetting = props.form.settings?.find(
				(s: any) => s.id === setting.id || s.key === setting.key
			) as any;
			
			return {
				...setting,
				value: taskSetting?.value ?? taskSetting?.pivot?.value ?? '',
			};
		});
	}

	async function handleSave() {
		if (!props.form.id || !settings.value.length) return;
		
		try {
			const payload: SettingPayload[] = settings.value.map((setting) => ({
				id: setting.id,
				value: setting.value,
			}));
			await updateTaskSettings(props.form.id, payload);
		} catch (e) {
			console.error('Failed to save settings:', e);
		}
	}
</script>
