<template>
	<BaseLayout no-copyright>
		<template #body>
			<div class="w-full items-center justify-between pt-6 md:flex md:py-4">
				<div class="rounded-xl p-2 dark:bg-gray-900 md:w-4/6">
					<div
						class="flex items-center justify-between rounded-xl border px-2 dark:border-0 dark:bg-gray-800"
					>
						<TextField
							class="w-5/6 p-2"
							placeholder="Enter your routine(enter to add new task)"
						/>

						<Button
							class="my-1 ml-2 w-32 rounded-xl py-2 px-2 text-sm text-white dark:bg-gray-800 dark:hover:bg-gray-700"
							type="submit"
						>
							<span class="relative">Never repeat</span>
						</Button>
					</div>
				</div>
				<ul class="m-4 flex justify-between md:mx-2">
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>15</span>
						<span class="text-gray-500">routines</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>1</span>
						<span class="text-gray-500">expired</span>
					</li>
					<li
						class="mx-2 flex w-28 flex-col rounded-xl border p-2 dark:border-0 dark:bg-gray-800"
					>
						<span>112</span>
						<span class="text-gray-500">compited</span>
					</li>
				</ul>
			</div>
			<ul class="w-full pt-4">
				<li
					class="m-2 flex justify-between rounded-xl border py-3.5 px-5 dark:border-0 dark:bg-gray-800"
				>
					<span>Task#1</span>
					<span>19:46</span>
				</li>
				<li
					class="m-2 flex justify-between rounded-xl border py-3.5 px-5 dark:border-0 dark:bg-gray-800"
				>
					<span>Task#2</span>
					<span>May 28</span>
				</li>
				<li
					class="m-2 flex justify-between rounded-xl border py-3.5 px-5 dark:border-0 dark:bg-gray-800"
				>
					<span>Task#3</span>
					<span>May 19 at 12:00</span>
				</li>
				<li
					class="m-2 flex justify-between rounded-xl border py-3.5 px-5 dark:border-0 dark:bg-gray-800"
				>
					<span>Task#4</span>
					<span>Every day at 13:25</span>
				</li>
			</ul>
		</template>
	</BaseLayout>
</template>

<script setup lang="ts">
	import TextField from 'src/components/general/TextField.vue';
	import Button from 'src/components/general/Button.vue';
</script>
