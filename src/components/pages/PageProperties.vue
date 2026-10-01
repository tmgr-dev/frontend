<template>
	<div
		class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2 text-sm"
		data-testid="page-properties"
	>
		<span
			class="inline-flex items-center rounded-full bg-gray-100 px-2.5 py-0.5 text-xs font-medium text-gray-700 dark:bg-gray-700 dark:text-gray-200"
			data-testid="page-type-badge"
			>{{ typeLabel }}</span
		>
		<span
			v-if="page.author"
			class="inline-flex items-center gap-1.5 text-ink-subtle"
			data-testid="page-author"
		>
			<span class="text-xs">Автор</span>
			<AuthorBadge :author="author" :size="18" />
		</span>
		<span
			v-if="page.updated_by"
			class="inline-flex items-center gap-1.5 text-ink-subtle"
			data-testid="page-updated-by"
		>
			<span class="text-xs">Изменил</span>
			<AuthorBadge :author="updatedBy" :size="18" />
			<span class="text-xs">{{ updatedAt }}</span>
		</span>

		<label
			v-for="def in defs"
			:key="def.key"
			class="inline-flex items-center gap-1.5"
		>
			<span class="text-xs text-ink-subtle">{{ def.label }}</span>
			<select
				v-if="def.kind === 'enum'"
				:value="page.properties?.[def.key] ?? ''"
				class="rounded border border-gray-300 bg-white px-2 py-0.5 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
				@change="update(def.key, ($event.target as HTMLSelectElement).value)"
			>
				<option value=""></option>
				<option v-for="o in def.options" :key="o.value" :value="o.value">
					{{ o.label }}
				</option>
			</select>
			<input
				v-else
				:type="def.kind === 'date' ? 'date' : 'text'"
				:value="page.properties?.[def.key] ?? ''"
				class="rounded border border-gray-300 bg-white px-2 py-0.5 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
				@change="update(def.key, ($event.target as HTMLInputElement).value)"
			/>
		</label>
	</div>
</template>

<script lang="ts">
	import type { Page } from '@/actions/tmgr/pages';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import { toAuthorRef } from '@/utils/pages/author';
	import { computed, defineComponent, type PropType } from 'vue';
	import { propertyDefsFor, TYPE_LABELS } from './propertyDefs';

	export default defineComponent({
		name: 'PageProperties',
		components: { AuthorBadge },
		props: {
			page: { type: Object as PropType<Page>, required: true },
		},
		emits: ['update'],
		setup(props, { emit }) {
			const defs = computed(() => propertyDefsFor(props.page.type));
			const typeLabel = computed(
				() => TYPE_LABELS[props.page.type] ?? props.page.type,
			);
			const author = computed(() => toAuthorRef(props.page.author));
			const updatedBy = computed(() => toAuthorRef(props.page.updated_by));
			const updatedAt = computed(() =>
				props.page.updated_at
					? new Date(props.page.updated_at).toLocaleString()
					: '',
			);
			const update = (key: string, value: string) =>
				emit('update', { ...props.page.properties, [key]: value || null });

			return { defs, typeLabel, author, updatedBy, updatedAt, update };
		},
	});
</script>
