<template>
	<div class="flex justify-end gap-3 text-center">
		<button
			v-if="!isCreatingTask"
			@click="$emit('removeTask')"
			class="mr-auto rounded bg-status-fix px-4 py-2 font-bold text-white outline-none hover:opacity-90"
		>
			Delete
		</button>

		<slot />

		<span class="relative inline-flex rounded-md shadow-sm">
			<button
				v-if="!isCreatingTask"
				@click="$emit('saveTask')"
				class="relative rounded bg-brand px-8 py-2 font-bold text-white hover:bg-brand-hover"
				type="button"
			>
				<svg
					v-if="isSaving"
					class="absolute left-1.5 top-2.5 inline h-5 w-5 animate-spin text-white"
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
				class="absolute right-0 top-0 -mt-1 mr-4 flex h-5 w-5"
			>
				<span
					class="absolute inline-flex h-full w-full animate-ping rounded-full bg-brand opacity-75"
				/>
				<span
					class="relative inline-flex h-5 w-5 rounded-full bg-status-progress"
				/>
			</span>
		</span>

		<button
			v-if="isCreatingTask"
			@click="$emit('createTask')"
			class="mb-5 rounded bg-brand px-4 py-2 font-bold text-white transition hover:bg-brand-hover sm:mb-0"
			type="button"
		>
			Create
		</button>

		<!--		<button
			v-if="isCreatingTask"
			@click="$emit('cancelCreateTask')"
			class="mb-5 rounded bg-surface-active px-4 py-2 font-bold text-ink transition hover:bg-surface-hover sm:mb-0"
			type="button"
		>
			Cancel
		</button>-->

		<button
			v-if="!isCreatingTask"
			@click="$emit('settingsTask')"
			class="rounded bg-surface-active px-4 py-2 font-bold text-ink hover:bg-surface-hover"
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
