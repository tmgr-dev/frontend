<template>
	<div class="flex flex-col items-center justify-center py-8 text-center">
		<component
			v-if="iconComponent"
			:is="iconComponent"
			class="mb-4 h-12 w-12 text-ink-subtle"
		/>
		<img
			v-else
			src="@/assets/img/two-people-by-taskboard.png"
			class="m-auto"
			width="420"
			height="296"
			alt=""
		/>
		<p class="mt-4 text-xl font-bold text-ink">
			{{ title || 'Nothing here yet' }}
		</p>
		<p v-if="description" class="mt-2 text-sm text-ink-muted">
			{{ description }}
		</p>
		<Button
			v-if="action"
			class="mt-6 h-11 px-8 text-base font-semibold"
			@click="action.onClick"
		>
			{{ action.label }}
		</Button>
	</div>
</template>

<script setup lang="ts">
	import { Button } from '@/components/ui/button';
	import { Activity, ClipboardList, Users } from 'lucide-vue-next';
	import { computed } from 'vue';

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
