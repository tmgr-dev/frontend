<template>
	<div class="flex flex-col items-center justify-center py-8 text-center">
		<component
			v-if="iconComponent"
			:is="iconComponent"
			class="mb-4 h-12 w-12 text-gray-400"
		/>
		<img
			v-else
			src="@/assets/img/two-people-by-taskboard.png"
			class="m-auto"
			width="420"
			height="296"
			alt=""
		/>
		<p class="mt-4 text-xl font-bold text-gray-700 dark:text-gray-300">
			{{ title || 'Nothing here yet' }}
		</p>
		<p v-if="description" class="mt-2 text-sm text-gray-500 dark:text-gray-400">
			{{ description }}
		</p>
		<button
			v-if="action"
			class="mt-6 inline-flex h-11 items-center rounded-md bg-tmgr-blue px-8 text-base font-semibold text-white shadow-sm transition-colors hover:bg-tmgr-blue/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tmgr-blue focus-visible:ring-offset-2 dark:bg-blue-600 dark:hover:bg-blue-500 dark:focus-visible:ring-blue-500 dark:ring-offset-gray-900"
			@click="action.onClick"
		>
			{{ action.label }}
		</button>
	</div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Activity, Users, ClipboardList } from 'lucide-vue-next';

interface ActionType {
	label: string;
	onClick: () => void;
}

interface Props {
	icon?: string;
	title?: string;
	description?: string;
	action?: ActionType;
}

const props = defineProps<Props>();

const iconMap: Record<string, typeof Activity> = {
	activity: Activity,
	users: Users,
	task: ClipboardList,
};

const iconComponent = computed(() => {
	if (!props.icon) return null;
	return iconMap[props.icon] || null;
});
</script>
