<script setup lang="ts">
	import { Category } from '@/actions/tmgr/categories';
	import store from '@/store';
	import { PlusCircleIcon } from '@heroicons/vue/24/outline';

	interface Props {
		category: Category;
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
			class="z-10 rounded bg-gray-700 p-1.5 text-xs !leading-none text-white lg:text-sm"
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
			class="z-20 hidden opacity-10 hover:opacity-100 group-hover:block"
			title="Create a task of this category"
		>
			<PlusCircleIcon class="h-4 w-4 lg:h-6 lg:w-6" />
		</button>
	</div>
</template>
