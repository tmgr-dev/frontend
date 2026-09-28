<template>
	<div
		class="flex flex-col gap-3 py-3 first:pt-0 last:pb-0 sm:flex-row sm:items-center sm:justify-between sm:gap-6"
		data-settings-row
	>
		<div class="min-w-0">
			<p :id="labelId" class="text-sm font-medium text-ink">
				<slot name="label">{{ label }}</slot>
			</p>
			<p
				v-if="description || $slots.description"
				:id="descriptionId"
				class="mt-0.5 text-sm text-ink-subtle"
			>
				<slot name="description">{{ description }}</slot>
			</p>
		</div>
		<div class="flex shrink-0 items-center gap-2">
			<slot :label-id="labelId" :description-id="descriptionId" />
		</div>
	</div>
</template>

<script>
	import { defineComponent } from 'vue';

	let sequence = 0;

	export default defineComponent({
		name: 'SettingsRow',
		props: {
			label: { type: String, default: '' },
			description: { type: String, default: '' },
			id: { type: String, default: '' },
		},
		setup(props) {
			const base = props.id || `settings-row-${++sequence}`;
			return {
				labelId: `${base}-label`,
				descriptionId: `${base}-description`,
			};
		},
	});
</script>
