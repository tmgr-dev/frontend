<script setup lang="ts">
	import { Category } from '@/actions/tmgr/categories';
	import store from '@/store';
	import { PlusCircleIcon } from '@heroicons/vue/24/outline';

	interface Props {
		category: Pick<Category, 'id' | 'title'> | null;
		statusId?: number;
	}

	const props = defineProps<Props>();
</script>

<template>
	<div v-if="props.category?.id" class="group flex items-center gap-1">
		<router-link
			:to="{
				name: 'ProjectCategoryChildrenList',
				params: { id: props.category.id },
			}"
			class="z-10 inline-flex items-center rounded-pill bg-tag-bg px-2 py-0.5 text-2xs !leading-none font-bold tracking-wide text-tag-fg uppercase"
		>
			{{ props.category.title }}
		</router-link>

		<button
			@click.stop="
				store.commit('createTaskInProjectCategoryId', {
					projectCategoryId: props.category.id,
					statusId: props.statusId,
				})
			"
			type="button"
			class="z-20 hidden text-ink-muted opacity-40 group-hover:block hover:opacity-100"
			title="Create a task of this category"
		>
			<PlusCircleIcon class="h-3.5 w-3.5" />
		</button>
	</div>
</template>
