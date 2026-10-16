<script setup lang="ts">
	import { WorkspaceMember } from '@/actions/tmgr/workspaces';
	import AssigneePersonaGroup from '@/components/general/AssigneePersonaGroup.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
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
	import { cn } from '@/utils';
	import { type PersonaAssignee } from '@/utils/personas';
	import { UserIcon } from '@heroicons/vue/24/outline';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { computed, ref } from 'vue';

	interface Props {
		assignees: WorkspaceMember[];
		modelValue: WorkspaceMember['id'];
		assignablePersonas?: PersonaAssignee[];
		selectedPersonas?: PersonaAssignee[];
	}
	const props = withDefaults(defineProps<Props>(), {
		assignablePersonas: () => [],
		selectedPersonas: () => [],
	});
	const emit = defineEmits<{
		togglePersona: [persona: PersonaAssignee];
	}>();
	const assigneeIds = defineModel<number[]>({
		default: () => [],
	});
	const impliedOwnerIds = computed(
		() => new Set(props.selectedPersonas.map((p) => p.owner.id)),
	);
	const selectedPersonaIds = computed(() =>
		props.selectedPersonas.map((p) => p.id),
	);
	const selectedMembers = computed(() =>
		props.assignees.filter(
			(assignee) =>
				assigneeIds.value.includes(assignee.id) &&
				!impliedOwnerIds.value.has(assignee.id),
		),
	);
	const MAX_SUMMARY_AVATARS = 3;
	const triggerLabel = computed(() => {
		const names = [
			...selectedMembers.value.map((assignee) => assignee.name),
			...props.selectedPersonas.map((persona) => persona.name),
		];
		return names.length > 0 ? names.join(', ') : 'Assignee';
	});
	const openCombobox = ref(false);
	const searchValue = ref('');
	const filteredAssignees = computed(() => {
		if (!searchValue.value) return props.assignees;

		return props.assignees.filter((assignee) =>
			assignee.name.toLowerCase().includes(searchValue.value.toLowerCase()),
		);
	});
</script>

<template>
	<div class="flex items-center gap-2">
		<UserIcon class="size-5" />

		<Popover v-model:open="openCombobox">
			<PopoverTrigger as-child>
				<Button
					variant="ghost"
					role="combobox"
					:aria-expanded="openCombobox"
					:aria-label="triggerLabel"
					class="w-32 justify-between overflow-hidden px-0"
				>
					<span
						v-if="selectedMembers.length"
						class="flex shrink-0 items-center -space-x-1.5"
						:title="triggerLabel"
					>
						<UserAvatar
							v-for="member in selectedMembers.slice(0, MAX_SUMMARY_AVATARS)"
							:key="member.id"
							:user-id="member.id"
							:name="member.name"
							:has-avatar="member.has_avatar ?? false"
							:size="20"
							class="ring-2 ring-white dark:ring-gray-800"
						/>
						<span
							v-if="selectedMembers.length > MAX_SUMMARY_AVATARS"
							class="relative z-10 pl-2.5 text-xs text-gray-600 dark:text-gray-400"
						>
							+{{ selectedMembers.length - MAX_SUMMARY_AVATARS }}
						</span>
					</span>
					<span
						class="truncate"
						:class="{ 'ml-1.5': selectedMembers.length }"
					>
						{{ triggerLabel }}
					</span>
					<ChevronsUpDown class="ml-1 h-4 w-4 shrink-0 opacity-50" />
				</Button>
			</PopoverTrigger>

			<PopoverContent
				class="w-52 rounded bg-white p-0 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400"
			>
				<Command>
					<CommandInput
						class="h-9"
						wrapper-class="dark:border-gray-600"
						placeholder="Search assignees..."
						@input="(e) => (searchValue = e.target.value)"
					/>
					<CommandEmpty>No assignees found.</CommandEmpty>
					<CommandList>
						<CommandGroup>
							<CommandItem
								v-for="assignee in filteredAssignees"
								:key="assignee.id"
								:value="assignee.id"
								@select="
									(e) => {
										if (
											typeof e.detail.value === 'number' &&
											!impliedOwnerIds.has(e.detail.value)
										) {
											if (assigneeIds.includes(e.detail.value)) {
												assigneeIds = assigneeIds.filter(
													(v) => v !== e.detail.value,
												);
											} else {
												assigneeIds.push(e.detail.value);
											}
											openCombobox = false;
										}
									}
								"
								class="cursor-pointer text-gray-900 hover:!bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
							>
								<UserAvatar
									:user-id="assignee.id"
									:name="assignee.name"
									:has-avatar="assignee.has_avatar ?? false"
									:size="20"
									class="mr-2"
								/>
								<span class="truncate">{{ assignee.name }}</span>
								<Check
									:class="
										cn(
											'ml-auto h-4 w-4',
											assigneeIds.includes(assignee.id)
												? 'opacity-100'
												: 'opacity-0',
										)
									"
								/>
							</CommandItem>
						</CommandGroup>
						<AssigneePersonaGroup
							:personas="assignablePersonas"
							:selected-ids="selectedPersonaIds"
							@toggle="(persona) => emit('togglePersona', persona)"
						/>
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	</div>
</template>
