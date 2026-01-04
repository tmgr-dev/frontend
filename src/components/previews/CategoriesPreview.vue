<template>
	<div class="p-6">
		<!-- Header -->
		<div class="mb-6 flex items-center justify-between">
			<div class="h-8 w-40 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="h-10 w-36 animate-pulse rounded bg-blue-200 dark:bg-blue-900"></div>
		</div>

		<!-- Categories grid -->
		<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
			<!-- Category cards -->
			<div v-for="i in 6" :key="i" class="group rounded-xl border border-gray-200 bg-white p-5 transition-shadow hover:shadow-lg dark:border-gray-700 dark:bg-gray-800">
				<!-- Header with color dot -->
				<div class="mb-3 flex items-center gap-3">
					<div 
						class="h-4 w-4 rounded-full"
						:class="getCategoryColor(i)"
					></div>
					<div class="h-5 flex-1 animate-pulse rounded bg-gray-200 dark:bg-gray-700" :class="getRandomWidth()"></div>
				</div>

				<!-- Description -->
				<div class="mb-4 space-y-1">
					<div class="h-3 w-full animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
					<div class="h-3 w-2/3 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
				</div>

				<!-- Stats -->
				<div class="flex items-center justify-between text-sm">
					<div class="flex items-center gap-2">
						<div class="h-5 w-16 animate-pulse rounded-full bg-blue-100 dark:bg-blue-900/30"></div>
						<div class="h-5 w-12 animate-pulse rounded-full bg-green-100 dark:bg-green-900/30"></div>
					</div>
					<div class="h-4 w-20 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
				</div>

				<!-- Progress bar -->
				<div class="mt-3">
					<div class="h-2 w-full overflow-hidden rounded-full bg-gray-100 dark:bg-gray-700">
						<div 
							class="h-full animate-pulse rounded-full"
							:class="getCategoryColor(i)"
							:style="{ width: `${30 + (i * 10) % 60}%` }"
						></div>
					</div>
				</div>
			</div>
		</div>

		<!-- Tree view hint -->
		<div class="mt-8 rounded-lg border border-dashed border-gray-300 bg-gray-50 p-6 text-center dark:border-gray-600 dark:bg-gray-800/50">
			<div class="mx-auto mb-3 h-12 w-12 animate-pulse rounded-full bg-gray-200 dark:bg-gray-700"></div>
			<div class="mx-auto h-4 w-48 animate-pulse rounded bg-gray-200 dark:bg-gray-700"></div>
			<div class="mx-auto mt-2 h-3 w-64 animate-pulse rounded bg-gray-100 dark:bg-gray-600"></div>
		</div>
	</div>
</template>

<script setup lang="ts">
const getCategoryColor = (index: number) => {
	const colors = [
		'bg-blue-500',
		'bg-green-500',
		'bg-purple-500',
		'bg-yellow-500',
		'bg-red-500',
		'bg-pink-500',
	];
	return colors[(index - 1) % colors.length];
};

const getRandomWidth = () => {
	const widths = ['w-1/2', 'w-2/3', 'w-3/4', 'w-full'];
	return widths[Math.floor(Math.random() * widths.length)];
};
</script>

