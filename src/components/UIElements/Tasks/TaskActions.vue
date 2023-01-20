<template>
	<div class="tc-block text-center flex justify-end">
		<button
			v-if="!isCreatingTask"
			@click="$emit('removeTask')"
			class="bg-red-500 mr-5 hover:bg-red-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline mr-auto"
		>
			Delete
		</button>

		<slot></slot>

		<span class="relative inline-flex rounded-md shadow-sm">
			<button-with-actions
				v-if="!isCreatingTask"
				:actions="[{action: () => $emit('saveTask'), label: 'Save'},{action: () => $emit('saveTask', true), label: 'Save & Start'}]"
				direction="up"
				color="blue"
				class="mr-5"
			/>
<!--			<button-->
<!--				v-if="!isCreatingTask"-->
<!--				@click="$emit('saveTask')"-->
<!--				class="bg-blue-500 mr-5 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"-->
<!--				type="button"-->
<!--			>-->
<!--				<svg-->
<!--					v-if="isSaving"-->
<!--					class="animate-spin -ml-1 mr-3 h-5 w-5 text-white inline"-->
<!--					xmlns="http://www.w3.org/2000/svg"-->
<!--					fill="none"-->
<!--					viewBox="0 0 24 24"-->
<!--				>-->
<!--					<circle-->
<!--						class="opacity-25"-->
<!--						cx="12"-->
<!--						cy="12"-->
<!--						r="10"-->
<!--						stroke="currentColor"-->
<!--						stroke-width="4"-->
<!--					></circle>-->
<!--					<path-->
<!--						class="opacity-75"-->
<!--						fill="currentColor"-->
<!--						d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"-->
<!--					></path>-->
<!--				</svg>-->

<!--				Save-->
<!--			</button>-->

			<span
				v-if="isDataEdited"
				class="flex absolute h-5 w-5 top-0 right-0 -mt-1 mr-4"
			>
				<span
					class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"
				></span>
				<span
					class="relative inline-flex rounded-full h-5 w-5 bg-yellow-500"
				></span>
			</span>
		</span>

		<button
			v-if="isCreatingTask"
			@click="$emit('createTask')"
			class="bg-orange-5 mr-5 hover:bg-orange-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
			type="button"
		>
			Create
		</button>

		<button
			v-if="isCreatingTask"
			@click="$store.dispatch('closeTaskModal')"
			class="bg-gray-500 hover:bg-gray-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline sm:mb-0 mb-5"
			type="button"
		>
			Cancel
		</button>

		<button
			v-if="!isCreatingTask"
			@click="$emit('settingsTask')"
			class="bg-gray-500 hover:bg-gray-700 text-white font-bold py-2 px-4 rounded focus:outline-none focus:shadow-outline"
			type="button"
		>
			Settings
		</button>
	</div>
</template>

<script>
	import ButtonWithActions from 'src/components/UIElements/ButtonWithActions.vue';

	export default {
		name: 'TaskActions',
		components: { ButtonWithActions },
		emits: ['createTask', 'saveTask', 'settingsTask', 'removeTask'],
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
