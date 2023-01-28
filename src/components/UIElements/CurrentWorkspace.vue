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
	<br>
	<a href="#" @click.prevent="isShowCreateWorkspaceInvitationModal = true" class="py-2 inline-block relative">Create invitation to workspace <span class="material-icons ml-2 mt-0 absolute inline-block">add_circle_outline</span></a>
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
	<Transition name="bounce-right-fade">
		<modal
			v-if="isShowCreateWorkspaceInvitationModal"
			:modal-class="`p-6 ${newWorkspaceInvitation.token ? 'w-full' : 'w-96'}`"
			close-on-bg-click
			@close="isShowCreateWorkspaceInvitationModal = false"
		>
			<template #modal-body>
				<div v-if="!newWorkspaceInvitation.token">
					<input-field
						v-model="newWorkspaceInvitation.max_usage_times"
						:errors="errors.max_usage_times"
						type="number"
						extra-class="mb-1 bg-white dark:bg-gray-800"
						placeholder="Max usage times"
					/>
					<input-field
						v-model="newWorkspaceInvitation.expired_at"
						:errors="errors.expired_at"
						type="datetime-local"
						extra-class="mb-1 bg-white dark:bg-gray-800"
						placeholder="Expired at"
					/>
					<button
						@click="createNewWorkspaceInvitation()"
						class="w-full bg-orange-500 mr-5 mt-5 hover:bg-orange-600 transition text-white font-bold py-2 px-4 rounded focus:outline-none sm:mb-0"
						type="button"
					>
						Create
					</button>
				</div>
				<div v-else>
					<a :href="invitationLink">{{ invitationLink }}</a>
				</div>
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
				isShowCreateWorkspaceInvitationModal: false,
				workspaces: [],
				errors: {},
				newWorkspace: {
					name: '',
					type: 'test'
				},
				newWorkspaceInvitation: {
					max_usage_times: '',
					expired_at: null
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
			invitationLink () {
				return `${document.location.protocol}//${document.location.host}/workspaces/invitations/${this.newWorkspaceInvitation.token}`
			}
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
			async createNewWorkspaceInvitation () {
				try {
					let {
						data: { data: newWorkspaceInvitation },
					} = await this.$axios.post(`/workspaces/${this.val}/invitations`, this.newWorkspaceInvitation)
					this.newWorkspaceInvitation = newWorkspaceInvitation
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
