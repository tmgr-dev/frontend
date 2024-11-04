<template>
	<Select
		v-model="val"
		:options="workspaces"
		label-key="name"
		value-key="id"
		@updateSettings="() => $emit('updateSettings')"
	/>

	<button
		@click="flags.isShowWorkspaceModal = true"
		class="relative flex items-end gap-2 py-2"
	>
		<span class="material-icons text-lg">add_circle_outline</span>
		Add new workspace
	</button>

	<Transition name="bounce-right-fade">
		<Modal
			v-if="flags.isShowWorkspaceModal"
			modal-class="p-6 w-96"
			close-on-bg-click
			@close="flags.isShowWorkspaceModal = false"
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
						:disabled="flags.isLoading"
						:class="{ 'bg-neutral-400 hover:bg-neutral-400': flags.isLoading }"
						class="mt-3 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white outline-none transition hover:bg-orange-600 sm:mb-0"
						type="button"
					>
						Create
					</button>
				</div>
			</template>
		</Modal>
	</Transition>

	<button
		@click="flags.isShowInvitationModal = true"
		class="relative flex items-end gap-2 py-2"
	>
		<span class="material-icons text-lg">add_circle_outline</span>
		Send invitation to workspace
	</button>

	<Transition name="bounce-right-fade">
		<Modal
			v-if="flags.isShowInvitationModal"
			:modal-class="`p-6 ${invitationToken ? 'w-auto' : 'w-96'}`"
			close-on-bg-click
			@close="closeInvitationModal"
		>
			<template #modal-body>
				<div v-if="!invitationToken">
					<!-- Email Invitations -->
					<label class="flex flex-col gap-2 mb-4">
						Email Addresses
						<textarea
							v-model="newWorkspaceInvitation.emails"
							rows="3"
							class="w-full rounded border border-gray-300 px-3 py-2"
							placeholder="email1@example.com, email2@example.com"
						></textarea>
						<span class="text-sm text-gray-500">
              Separate multiple emails with commas
            </span>
					</label>

					<!-- Usage Limit (only show if no emails) -->
					<label v-if="!newWorkspaceInvitation.emails" class="flex flex-col gap-2">
						Max usage times
						<TextField
							type="number"
							v-model="newWorkspaceInvitation.max_usage_times"
							:errors="errors.max_usage_times"
							placeholder="Leave empty for unlimited"
						/>
					</label>

<!--					&lt;!&ndash; Expiration Date &ndash;&gt;-->
<!--					<label class="mt-3 flex flex-col gap-2">-->
<!--						Expired at-->
<!--						<TextField-->
<!--							type="datetime-local"-->
<!--							v-model="newWorkspaceInvitation.expired_at"-->
<!--							:errors="errors.expired_at"-->
<!--							placeholder="Expired at"-->
<!--						/>-->
<!--					</label>-->

					<button
						@click="createNewWorkspaceInvitation()"
						:disabled="flags.isLoading"
						:class="{ 'bg-neutral-400 hover:bg-neutral-400': flags.isLoading }"
						class="mr-5 mt-5 w-full rounded bg-orange-500 py-2 px-4 font-bold text-white transition hover:bg-orange-600 focus:outline-none sm:mb-0"
						type="button"
					>
						{{ flags.isLoading ? 'Creating...' : 'Send Invitation' }}
					</button>
				</div>

				<!-- Success State - Show Link -->
				<div v-else>
					<div class="mb-4">
						<h3 class="font-bold text-lg mb-2">Invitation sent!</h3>
						<p v-if="newWorkspaceInvitation.emails" class="text-sm text-gray-600 mb-4">
							Invitations have been sent to the specified email addresses.
						</p>
						<p v-else class="text-sm text-gray-600 mb-4">
							You can share this link with anyone you want to invite:
						</p>
					</div>

					<div v-if="!newWorkspaceInvitation.emails" class="flex gap-2 bg-gray-50 p-3 rounded">
						<code class="text-sm break-all">
							{{ invitationLink }}
						</code>

						<button
							@click="copyToClipboard(invitationLink)"
							class="flex-shrink-0"
							:title="flags.isCopied ? 'Copied!' : 'Copy to clipboard'"
						>
              <span
								class="material-icons transition-colors"
								:class="{ 'text-green-500': flags.isCopied }"
							>
                content_copy
              </span>
						</button>
					</div>
				</div>
			</template>
		</Modal>
	</Transition>
</template>

<script setup lang="ts">
	import { copyToClipboard as copy } from 'quasar';
	import {
		createWorkspace,
		createWorkspaceInvitation,
		getWorkspaces,
		Workspace,
		WorkspaceInvitation,
	} from 'src/actions/tmgr/workspaces';
	import Select from 'src/components/general/Select.vue';
	import TextField from 'src/components/general/TextField.vue';
	import { computed, onBeforeMount, reactive, Ref, ref } from 'vue';
	import { AxiosError } from 'axios';
	import Modal from 'src/components/Modal.vue';

	interface Props {
		modelValue: string | number;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['update:modelValue', 'updateSettings']);

	const flags = reactive({
		isLoading: false,
		isCopied: false,
		isShowInvitationModal: false,
		isShowWorkspaceModal: false,
	});
	const workspaces: Ref<Workspace[]> = ref([]);
	const errors = ref({});
	const newWorkspace: Ref<Workspace> = ref({
		name: '',
		type: 'test',
	});
	const newWorkspaceInvitation: Ref<WorkspaceInvitation> = ref({
		max_usage_times: '',
		expired_at: null,
		emails: '',
	});
	const invitationToken = ref('');

	onBeforeMount(async () => {
		workspaces.value = await getWorkspaces();
	});

	const val = computed({
		get() {
			return props.modelValue;
		},
		set(v) {
			emit('update:modelValue', v);
		},
	});

	const invitationLink = computed(() => {
		return `${location.protocol}//${location.host}/workspaces/invitations/${invitationToken.value}`;
	});

	function closeInvitationModal() {
		flags.isShowInvitationModal = false;
		invitationToken.value = '';
		newWorkspaceInvitation.value = {
			max_usage_times: '',
			expired_at: null,
			emails: '',
		};
	}

	async function copyToClipboard(value: string) {
		try {
			await copy(value);
			flags.isCopied = true;

			setTimeout(() => {
				flags.isCopied = false;
			}, 2000);
		} catch (e) {
			console.warn('failed to copy', e);
		}
	}

	async function createNewWorkspace() {
		try {
			flags.isLoading = true;
			errors.value = {};
			const workspace = await createWorkspace(newWorkspace.value);
			workspaces.value.push(workspace);
			flags.isShowWorkspaceModal = false;
			window.location.reload();
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
			}
		} finally {
			flags.isLoading = false;
		}
	}

	async function createNewWorkspaceInvitation() {
		try {
			flags.isCopied = false;
			flags.isLoading = true;
			errors.value = {};

			const response = await createWorkspaceInvitation(
				+val.value,
				newWorkspaceInvitation.value,
			);

			// If it's a general invitation (no emails), store the token
			if (!newWorkspaceInvitation.value.emails) {
				invitationToken.value = response.token;
			} else {
				// For email invitations, just show success message
				invitationToken.value = 'sent';
			}
		} catch (error) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
			}
		} finally {
			flags.isLoading = false;
		}
	}
</script>
