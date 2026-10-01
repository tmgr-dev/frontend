<script setup lang="ts">
	import { WorkspaceMember } from '@/actions/tmgr/workspaces';
	import AssigneePersonaGroup from '@/components/general/AssigneePersonaGroup.vue';
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
		default: [],
	});
	const impliedOwnerIds = computed(
		() => new Set(props.selectedPersonas.map((p) => p.owner.id)),
	);
	const selectedPersonaIds = computed(() =>
		props.selectedPersonas.map((p) => p.id),
	);
	const triggerLabel = computed(() => {
		const names = [
			...props.assignees
				.filter(
					(assignee) =>
						assigneeIds.value.includes(assignee.id) &&
						!impliedOwnerIds.value.has(assignee.id),
				)
				.map((assignee) => assignee.name),
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
					class="w-32 justify-between overflow-hidden px-0"
				>
					<span class="truncate">
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
								{{ assignee.name }}
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
