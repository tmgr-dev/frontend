<template>
	<div class="space-y-1.5" data-testid="aliases-editor">
		<div
			v-for="(alias, index) in modelValue"
			:key="index"
			class="flex flex-wrap items-start gap-1.5"
		>
			<select
				:value="alias.source"
				class="rounded border bg-white px-2 py-0.5 text-sm text-gray-900 dark:bg-gray-800 dark:text-gray-100"
				:class="fieldClass(index, 'source')"
				@change="
					patch(index, 'source', ($event.target as HTMLSelectElement).value)
				"
			>
				<option v-for="s in sources" :key="s.value" :value="s.value">
					{{ s.label }}
				</option>
			</select>
			<input
				type="text"
				:value="alias.native_id"
				placeholder="Identifier"
				class="w-36 rounded border bg-white px-2 py-0.5 text-sm text-gray-900 dark:bg-gray-800 dark:text-gray-100"
				:class="fieldClass(index, 'native_id')"
				@change="
					patch(index, 'native_id', ($event.target as HTMLInputElement).value)
				"
			/>
			<input
				type="text"
				:value="alias.display"
				placeholder="Display name"
				class="w-36 rounded border bg-white px-2 py-0.5 text-sm text-gray-900 dark:bg-gray-800 dark:text-gray-100"
				:class="fieldClass(index, 'display')"
				@change="
					patch(index, 'display', ($event.target as HTMLInputElement).value)
				"
			/>
			<button
				type="button"
				class="rounded p-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
				aria-label="Remove alias"
				@click="remove(index)"
			>
				<X class="h-3.5 w-3.5" />
			</button>
		</div>
		<button
			type="button"
			class="inline-flex items-center gap-1 text-xs text-blue-600 hover:underline dark:text-blue-400"
			data-testid="alias-add"
			@click="add"
		>
			<Plus class="h-3 w-3" />
			Add alias
		</button>
	</div>
</template>

<script lang="ts">
	import type { PersonAlias } from '@/actions/tmgr/pages';
	import {
		ALIAS_SOURCES,
		emptyAlias,
		type PropertyErrors,
	} from '@/utils/pages/properties';
	import { Plus, X } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AliasesEditor',
		components: { Plus, X },
		props: {
			modelValue: {
				type: Array as PropType<PersonAlias[]>,
				default: () => [],
			},
			errors: { type: Object as PropType<PropertyErrors>, default: () => ({}) },
		},
		emits: ['update:modelValue'],
		setup(props, { emit }) {
			const patch = (index: number, field: keyof PersonAlias, value: string) =>
				emit(
					'update:modelValue',
					props.modelValue.map((alias, i) =>
						i === index ? { ...alias, [field]: value } : { ...alias },
					),
				);
			const add = () =>
				emit('update:modelValue', [
					...props.modelValue.map((alias) => ({ ...alias })),
					emptyAlias(),
				]);
			const remove = (index: number) =>
				emit(
					'update:modelValue',
					props.modelValue
						.filter((_, i) => i !== index)
						.map((alias) => ({ ...alias })),
				);
			const fieldClass = (index: number, field: string) =>
				props.errors[`aliases.${index}.${field}`]
					? 'border-red-500'
					: 'border-gray-300 dark:border-gray-600';
			return { sources: ALIAS_SOURCES, patch, add, remove, fieldClass };
		},
	});
</script>
