<template>
	<Select
		v-model="val"
		:options="workspaces"
		label-key="name"
		value-key="id"
		@updateSettings="() => $emit('updateSettings')"
	/>

	<button
		@click="isShowWorkspaceModal = true"
		class="relative flex items-end gap-2 py-2"
	>
		<span class="material-icons text-lg">add_circle_outline</span>
		Add new workspace
	</button>

	<Transition name="bounce-right-fade">
		<modal
			v-if="isShowWorkspaceModal"
			modal-class="p-6 w-96"
			close-on-bg-click
			@close="isShowWorkspaceModal = false"
		>
			<template #modal-body>
				<TextField
					v-model="newWorkspace.name"
					:errors="errors.name"
					placeholder="New workspace name"
				/>
				<div class="flex">
					<button
						@click="createNewWorkspace()"
						:disabled="isLoading"
						:class="{ 'bg-neutral-400 hover:bg-neutral-400': isLoading }"
						class="mr-5 mt-5 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
						type="button"
					>
						Create
					</button>
					<button
						@click="isShowWorkspaceModal = false"
						:disabled="isLoading"
						:class="{ 'bg-neutral-400 hover:bg-neutral-400': isLoading }"
						class="mt-5 w-full rounded bg-gray-500 py-2 px-4 font-bold text-white transition hover:bg-gray-600 focus:outline-none sm:mb-0"
						type="button"
					>
						Cancel
					</button>
				</div>
			</template>
		</modal>
	</Transition>

	<button
		@click="isShowInvitationModal = true"
		class="relative flex items-end gap-2 py-2"
	>
		<span class="material-icons text-lg"> add_circle_outline </span>
		Create invitation to workspace
	</button>

	<Transition name="bounce-right-fade">
		<modal
			v-if="isShowInvitationModal"
			:modal-class="`p-6 ${newWorkspaceInvitation.token ? 'w-auto' : 'w-96'}`"
			close-on-bg-click
			@close="isShowInvitationModal = false"
		>
			<template #modal-body>
				<div v-if="!newWorkspaceInvitation.token">
					<label class="flex flex-col gap-2">
						Max usage times

						<TextField
							type="number"
							v-model="newWorkspaceInvitation.max_usage_times"
							:errors="errors.max_usage_times"
							placeholder="1"
						/>
					</label>

					<label class="mt-3 flex flex-col gap-2">
						Expired at

						<TextField
							type="datetime-local"
							v-model="newWorkspaceInvitation.expired_at"
							:errors="errors.expired_at"
							placeholder="Expired at"
						/>
					</label>

					<button
						@click="createNewWorkspaceInvitation()"
						:disabled="isLoading"
						:class="{ 'bg-neutral-400 hover:bg-neutral-400': isLoading }"
						class="mr-5 mt-5 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
						type="button"
					>
						Create
					</button>
				</div>

				<div v-else class="flex gap-2">
					<code>
						{{ invitationLink }}
					</code>

					<button @click="copyToClipboard(invitationLink)">
						<span
							class="material-icons transition-colors"
							:class="{ 'text-green-500': isCopied }"
						>
							content_copy
						</span>
					</button>
				</div>
			</template>
		</modal>
	</Transition>
</template>

<script>
	import { copyToClipboard as copy } from 'quasar';
	import {
		createWorkspace,
		createWorkspaceInvitation,
		getWorkspaces,
	} from 'src/actions/tmgr/workspaces';
	import Select from 'src/components/general/Select.vue';
	import TextField from 'src/components/general/TextField.vue';

	export default {
		name: 'CurrentWorkspace',
		components: {
			TextField,
			Select,
		},
		props: {
			modelValue: {
				required: false,
			},
		},

		emits: ['update:modelValue', 'updateSettings'],
		data() {
			return {
				isLoading: false,
				isShowWorkspaceModal: false,
				isShowInvitationModal: false,
				workspaces: [],
				errors: {},
				newWorkspace: {
					name: '',
					type: 'test',
				},
				newWorkspaceInvitation: {
					max_usage_times: '',
					expired_at: null,
				},
				isCopied: false,
			};
		},
		computed: {
			val: {
				get() {
					return this.modelValue;
				},
				set(v) {
					return this.$emit('update:modelValue', v);
				},
			},
			invitationLink() {
				return `${location.protocol}//${location.host}/workspaces/invitations/${this.newWorkspaceInvitation.token}`;
			},
		},
		methods: {
			async copyToClipboard(value) {
				try {
					await copy(value);
					this.isCopied = true;

					setTimeout(() => {
						this.isCopied = false;
					}, 2000);
				} catch (e) {
					console.warn('failed to copy', e);
				}
			},
			async createNewWorkspace() {
				try {
					this.isLoading = true;
					this.errors = {};
					const workspace = await createWorkspace(this.newWorkspace);
					this.workspaces.push(workspace);
					this.isShowWorkspaceModal = false;
				} catch (e) {
					this.errors = e.response?.data?.errors || {};
				} finally {
					this.isLoading = false;
				}
			},
			async createNewWorkspaceInvitation() {
				try {
					this.isCopied = false;
					this.isLoading = true;
					this.errors = {};
					this.newWorkspaceInvitation = await createWorkspaceInvitation(
						this.val,
						this.newWorkspaceInvitation,
					);
				} catch (e) {
					this.errors = e.response?.data?.errors || {};
				} finally {
					this.isLoading = false;
				}
			},
		},
		async created() {
			this.workspaces = await getWorkspaces();
		},
	};
</script>
