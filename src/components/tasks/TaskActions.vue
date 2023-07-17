<template>
	<div class="flex justify-end text-center">
		<button
			v-if="!isCreatingTask"
			@click="$emit('removeTask')"
			class="mr-5 mr-auto rounded bg-red-500 py-2 px-4 font-bold text-white outline-none hover:bg-red-700"
		>
			Delete
		</button>

		<slot />

		<span class="relative inline-flex rounded-md shadow-sm">
			<button
				v-if="!isCreatingTask"
				@click="$emit('saveTask')"
				class="relative mr-4 rounded bg-blue-500 py-2 px-8 font-bold text-white hover:bg-blue-700 focus:outline-none"
				type="button"
			>
				<svg
					v-if="isSaving"
					class="absolute top-2.5 left-1.5 inline h-5 w-5 animate-spin text-white"
					xmlns="http://www.w3.org/2000/svg"
					fill="none"
					viewBox="0 0 24 24"
				>
					<circle
						class="opacity-25"
						cx="12"
						cy="12"
						r="10"
						stroke="currentColor"
						stroke-width="4"
					/>
					<path
						class="opacity-75"
						fill="currentColor"
						d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
					/>
				</svg>
				<span>Save</span>
			</button>

			<span
				v-if="isDataEdited"
				class="absolute top-0 right-0 -mt-1 mr-4 flex h-5 w-5"
			>
				<span
					class="absolute inline-flex h-full w-full animate-ping rounded-full bg-blue-400 opacity-75"
				/>
				<span class="relative inline-flex h-5 w-5 rounded-full bg-yellow-500" />
			</span>
		</span>

		<button
			v-if="isCreatingTask"
			@click="$emit('createTask')"
			class="mr-5 mb-5 rounded bg-orange-500 py-2 px-4 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
			type="button"
		>
			Create
		</button>

		<button
			v-if="isCreatingTask"
			@click="$emit('cancelCreateTask')"
			class="mb-5 rounded bg-gray-500 py-2 px-4 font-bold text-white transition hover:bg-gray-600 focus:outline-none sm:mb-0"
			type="button"
		>
			Cancel
		</button>

		<button
			v-if="!isCreatingTask"
			@click="$emit('settingsTask')"
			class="rounded bg-gray-500 py-2 px-4 font-bold text-white hover:bg-gray-700 focus:outline-none"
			type="button"
		>
			Settings
		</button>
	</div>
</template>

<script>
	export default {
		name: 'TaskActions',
		emits: [
			'createTask',
			'saveTask',
			'settingsTask',
			'removeTask',
			'cancelCreateTask',
		],
		props: {
			isCreatingTask: {
				type: Boolean,
				required: false,
				default: false,
			},
			isSaving: {
				type: Boolean,
				required: false,
				default: false,
			},
			isDataEdited: {
				type: Boolean,
				required: false,
				default: false,
			},
		},
	};
</script>
