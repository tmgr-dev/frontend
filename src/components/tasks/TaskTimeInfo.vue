<template>
	<span
		v-if="formattedCreatedAtTime"
		class="flex items-center gap-1"
		:title="'Created: ' + formattedCreatedAtTime"
	>
		<ClockPlus class="size-2.5" />
		{{ formattedCreatedAtTime }}
	</span>

	<span
		v-if="
			formattedUpdatedAtTime &&
			formattedUpdatedAtTime !== formattedCreatedAtTime
		"
		class="flex items-center gap-1"
		:title="'Edited: ' + formattedUpdatedAtTime"
	>
		<FilePenLine class="size-2.5" />
		{{ formattedUpdatedAtTime }}
	</span>

	<span
		v-if="overtime"
		class="flex items-center gap-1 text-red-500 dark:text-red-400"
		title="Overtime"
	>
		<Siren class="size-2.5" />
		+{{ overtime }}
	</span>
</template>

<script setup lang="ts">
	import { ClockPlus, FilePenLine, Siren } from 'lucide-vue-next';
	import { formatRelativeTime } from '@/utils/timeUtils';
	import { computed } from 'vue';

	interface Props {
		createdAt?: string;
		updatedAt?: string;
		overtime?: string;
	}

	const props = defineProps<Props>();

	const formattedCreatedAtTime = computed(() =>
		formatRelativeTime(props.createdAt),
	);
	const formattedUpdatedAtTime = computed(() =>
		formatRelativeTime(props.updatedAt),
	);
</script>
