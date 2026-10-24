<template>
	<!--		<Select
		v-model="modelValue"
		:options="workspaces"
		label-key="name"
		modelValue-key="id"
	/>-->
	<Popover v-model:open="openCombobox">
		<PopoverTrigger as-child>
			<Button
				variant="outline"
				role="combobox"
				size="lg"
				:aria-expanded="openCombobox"
				class="w-full justify-between overflow-hidden px-4"
			>
				<span class="truncate">
					{{
						modelValue
							? workspaces.find((workspace) => workspace.id == modelValue)?.name
							: 'Workspace'
					}}
				</span>
				<ChevronsUpDown class="ml-1 h-4 w-4 shrink-0 opacity-50" />
			</Button>
		</PopoverTrigger>

		<PopoverContent class="rounded border-line bg-surface p-0 text-ink">
			<Command>
				<CommandInput
					class="h-9"
					wrapper-class="border-line"
					placeholder="Search category..."
					@input="(e) => (searchValue = e.target.value)"
				/>
				<CommandEmpty>No category found.</CommandEmpty>
				<CommandList class="w-80">
					<CommandGroup>
						<CommandItem
							v-for="workspace in filteredWorkspaces"
							:key="workspace.id"
							:value="workspace.id"
							@select="
								(e) => {
									if (e.detail.value) {
										modelValue = e.detail.value as number;
										searchValue = '';
									}
									openCombobox = false;
								}
							"
							class="cursor-pointer text-ink hover:!bg-brand hover:!text-on-accent"
						>
							{{ workspace.name }}
							<Check
								:class="
									cn(
										'ml-auto h-4 w-4',
										modelValue === workspace.id ? 'opacity-100' : 'opacity-0',
									)
								"
							/>
						</CommandItem>
					</CommandGroup>
				</CommandList>
			</Command>
		</PopoverContent>
	</Popover>

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
						:class="{ 'bg-ink-faint hover:bg-ink-faint': flags.isLoading }"
						class="mt-3 w-full rounded bg-brand px-4 py-2 font-bold text-on-accent outline-hidden transition hover:bg-brand-hover sm:mb-0"
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
			@close="closeInvitationModal"
		>
			<template #modal-body>
				<div v-if="!invitationToken">
					<!-- Email Invitations -->
					<label class="mb-4 flex flex-col gap-2">
						Email Addresses
						<textarea
							v-model="newWorkspaceInvitation.emails"
							rows="3"
							class="w-full rounded border border-input bg-background px-3 py-2 text-ink"
							placeholder="email1@example.com, email2@example.com"
						></textarea>
						<span class="text-sm text-ink-subtle">
							Separate multiple emails with commas
						</span>
					</label>

					<button
						@click="createNewWorkspaceInvitation()"
						:disabled="flags.isLoading"
						:class="{ 'bg-ink-faint hover:bg-ink-faint': flags.isLoading }"
						class="mr-5 mt-5 w-full rounded bg-brand px-4 py-2 font-bold text-on-accent transition hover:bg-brand-hover sm:mb-0"
						type="button"
					>
						{{ flags.isLoading ? 'Creating...' : 'Send Invitation' }}
					</button>
				</div>

				<!-- Success State - Show Link -->
				<div v-else>
					<div class="mb-4">
						<h3 class="mb-2 text-lg font-bold">Invitation sent!</h3>
						<p
							v-if="newWorkspaceInvitation.emails"
							class="mb-4 text-sm text-ink-muted"
						>
							Invitations have been sent to the specified email addresses.
						</p>
						<p v-else class="mb-4 text-sm text-ink-muted">
							You can share this link with anyone you want to invite:
						</p>
					</div>

					<div
						v-if="!newWorkspaceInvitation.emails"
						class="flex gap-2 rounded bg-surface-sunken p-3"
					>
						<code class="break-all text-sm">
							{{ invitationLink }}
						</code>

						<button
							@click="copyToClipboard(invitationLink)"
							class="shrink-0"
							:title="flags.isCopied ? 'Copied!' : 'Copy to clipboard'"
						>
							<span
								class="material-icons transition-colors"
								:class="{ 'text-status-done-fg': flags.isCopied }"
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
	import {
		createWorkspace,
		createWorkspaceInvitation,
		getWorkspaces,
		Workspace,
		WorkspaceInvitation,
	} from '@/actions/tmgr/workspaces';
	import TextField from '@/components/general/TextField.vue';
	import Modal from '@/components/Modal.vue';
	import { Button } from '@/components/ui/button';
	import {
		Command,
		CommandEmpty,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from '@/components/ui/command';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard.ts';
	import { cn } from '@/utils';
	import { AxiosError } from 'axios';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { computed, onBeforeMount, reactive, Ref, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';

	interface Props {
		modelValue: string | number;
	}

	type InvitationForm = Omit<WorkspaceInvitation, 'max_usage_times'> & {
		max_usage_times: number | '';
	};

	defineProps<Props>();
	const modelValue = defineModel<number>();
	const emit = defineEmits(['update:modelValue', 'updateSettings']);
	const route = useRoute();

	const flags = reactive({
		isLoading: false,
		isCopied: false,
		isShowInvitationModal: false,
		isShowWorkspaceModal: false,
	});
	const openCombobox = ref(false);
	const searchValue = ref('');
	const workspaces: Ref<Workspace[]> = ref([]);
	const filteredWorkspaces = computed(() => {
		if (!searchValue.value) return workspaces.value;

		return workspaces.value.filter((workspace) =>
			workspace.name.toLowerCase().includes(searchValue.value.toLowerCase()),
		);
	});

	const errors = ref<Record<string, string[]>>({});
	const newWorkspace: Ref<{ name: string; type: string }> = ref({
		name: '',
		type: 'test',
	});
	const newWorkspaceInvitation: Ref<InvitationForm> = ref({
		max_usage_times: '',
		expired_at: null,
		emails: '',
	});
	const invitationToken = ref('');
	const [copy] = useCopyToClipboard();
	watch(
		() => route.query['add-workspace'],
		async (addWorkspace) => {
			flags.isShowWorkspaceModal = addWorkspace === 'open';
			delete route.query['add-workspace'];
		},
		{
			deep: true,
		},
	);
	onBeforeMount(async () => {
		workspaces.value = await getWorkspaces();
		flags.isShowWorkspaceModal = route.query['add-workspace'] === 'open';
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
			// window.location.reload();
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
				Number(modelValue.value),
				// the form keeps '' for an empty limit; the API type says number
				newWorkspaceInvitation.value as WorkspaceInvitation,
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
