<template>
	<div class="p-6">
		<!-- Header -->
		<div class="mb-6">
			<div class="h-8 w-48 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
		</div>

		<!-- Stats cards -->
		<div class="mb-8 grid grid-cols-1 gap-4 md:grid-cols-4">
			<div class="rounded-xl bg-gradient-to-br from-blue-500 to-blue-600 p-6 text-white">
				<div class="text-sm opacity-80">Total Tasks</div>
				<div class="mt-2 h-10 w-16 animate-pulse rounded bg-white/20"></div>
			</div>
			<div class="rounded-xl bg-gradient-to-br from-green-500 to-green-600 p-6 text-white">
				<div class="text-sm opacity-80">Completed</div>
				<div class="mt-2 h-10 w-20 animate-pulse rounded bg-white/20"></div>
			</div>
			<div class="rounded-xl bg-gradient-to-br from-yellow-500 to-orange-500 p-6 text-white">
				<div class="text-sm opacity-80">In Progress</div>
				<div class="mt-2 h-10 w-14 animate-pulse rounded bg-white/20"></div>
			</div>
			<div class="rounded-xl bg-gradient-to-br from-purple-500 to-purple-600 p-6 text-white">
				<div class="text-sm opacity-80">Time Tracked</div>
				<div class="mt-2 h-10 w-24 animate-pulse rounded bg-white/20"></div>
			</div>
		</div>

		<!-- Charts row -->
		<div class="mb-8 grid grid-cols-1 gap-6 lg:grid-cols-2">
			<!-- Activity chart -->
			<div class="rounded-xl border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
				<div class="mb-4 h-5 w-32 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
				<div class="flex h-48 items-end justify-between gap-2">
					<div class="h-1/4 w-8 animate-pulse rounded-t bg-blue-200 dark:bg-blue-900"></div>
					<div class="h-2/4 w-8 animate-pulse rounded-t bg-blue-200 dark:bg-blue-900"></div>
					<div class="h-3/4 w-8 animate-pulse rounded-t bg-blue-300 dark:bg-blue-800"></div>
					<div class="h-full w-8 animate-pulse rounded-t bg-blue-400 dark:bg-blue-700"></div>
					<div class="h-2/3 w-8 animate-pulse rounded-t bg-blue-300 dark:bg-blue-800"></div>
					<div class="h-1/2 w-8 animate-pulse rounded-t bg-blue-200 dark:bg-blue-900"></div>
					<div class="h-3/4 w-8 animate-pulse rounded-t bg-blue-300 dark:bg-blue-800"></div>
				</div>
			</div>

			<!-- Heatmap -->
			<div class="rounded-xl border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
				<div class="mb-4 h-5 w-40 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
				<div class="grid grid-cols-12 gap-1">
					<template v-for="i in 84" :key="i">
						<div 
							class="h-4 w-4 animate-pulse rounded"
							:class="getHeatmapColor(i)"
						></div>
					</template>
				</div>
			</div>
		</div>

		<!-- Recent activity -->
		<div class="rounded-xl border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
			<div class="mb-4 h-5 w-36 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="space-y-4">
				<div v-for="i in 4" :key="i" class="flex items-center gap-4">
					<div class="h-10 w-10 animate-pulse rounded-full bg-gray-200 dark:bg-gray-700"></div>
					<div class="flex-1">
						<div class="mb-1 h-4 w-3/4 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
						<div class="h-3 w-1/4 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					</div>
					<div class="h-3 w-20 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
const getHeatmapColor = (index: number) => {
	const colors = [
		'bg-gray-100 dark:bg-gray-700',
		'bg-green-100 dark:bg-green-900/30',
		'bg-green-200 dark:bg-green-800/40',
		'bg-green-300 dark:bg-green-700/50',
		'bg-green-400 dark:bg-green-600/60',
	];
	return colors[Math.floor(Math.random() * colors.length)];
};
</script>

