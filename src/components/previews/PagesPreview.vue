<template>
	<div class="mx-auto flex w-full max-w-5xl gap-6 p-6">
		<div class="w-56 shrink-0 space-y-2">
			<div
				v-for="width in [70, 55, 80, 45, 60]"
				:key="width"
				class="h-5 animate-pulse rounded bg-gray-200 dark:bg-gray-700"
				:style="{ width: `${width}%` }"
			></div>
		</div>
		<div class="flex-1 space-y-4 rounded-lg bg-white p-6 shadow-sm dark:bg-gray-800">
			<div class="h-8 w-2/3 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div
				v-for="i in 6"
				:key="i"
				class="h-4 animate-pulse rounded bg-gray-200 dark:bg-gray-700"
				:style="{ width: `${95 - i * 7}%` }"
			></div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';

	export default defineComponent({ name: 'PagesPreview' });
</script>
