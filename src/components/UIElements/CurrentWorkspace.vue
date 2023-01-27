<template>
	<input-field
		v-if="workspaces.length > 0"
		v-model="val"
		:options="workspaces"
		option-name-key="name"
		option-value-key="id"
		selected
		type="select"
	></input-field>
	<a href="#" @click.prevent="isShowCreateNewWorkspaceModal = true" class="py-2 inline-block relative">Add new workspace <span class="material-icons ml-2 mt-0 absolute inline-block">add_circle_outline</span></a>
	<Transition name="bounce-right-fade">
		<modal
			v-if="isShowCreateNewWorkspaceModal"
			modal-class="p-6 w-96"
			close-on-bg-click
			@close="isShowCreateNewWorkspaceModal = false"
		>
			<template #modal-body>
				<input-field
					v-model="newWorkspace.name"
					:errors="errors.title"
					type="text"
					extra-class="mb-1 bg-white dark:bg-gray-800"
					placeholder="New workspace name"
				/>
				<button
					@click="createNewWorkspace()"
					class="w-full bg-orange-500 mr-5 mt-5 hover:bg-orange-600 transition text-white font-bold py-2 px-4 rounded focus:outline-none sm:mb-0"
					type="button"
				>
					Create
				</button>
			</template>
		</modal>
	</Transition>
</template>

<script>
	import InputField from 'components/UIElements/InputField';

	export default {
		name: 'CurrentWorkspace',
		components: { InputField },
		props: {
			modelValue: {
				required: false,
			},
		},
		emits: ['update:modelValue'],
		data() {
			return {
				isShowCreateNewWorkspaceModal: false,
				workspaces: [],
				errors: {},
				newWorkspace: {
					name: '',
					type: 'test'
				}
			};
		},
		computed: {
			val: {
				get() {
					return parseInt(this.modelValue);
				},
				set(v) {
					return this.$emit('update:modelValue', v);
				},
			},
		},
		methods: {
			async createNewWorkspace () {
				try {
					let {
						data: { data: workspaces },
					} = await this.$axios.post('/workspaces', this.newWorkspace)

				} catch (e) {
					if (e.response && e.response && e.response.data.errors) {
						this.errors = e.response.data.errors;
					}
				}
			},
		},
		async created() {
			let {
				data: { data: workspaces },
			} = await this.$axios.get('/workspaces');
			this.workspaces = workspaces;
		},
	};
</script>

<style lang="scss" scoped></style>
