<script setup lang="ts">
	import { cn } from 'src/utils';
	import {
		Command,
		CommandEmpty,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from 'src/components/ui/command';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from 'src/components/ui/popover';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { Button } from 'src/components/ui/button';
	import { UserIcon } from '@heroicons/vue/24/outline';
	import { ref, computed } from 'vue';
	import { WorkspaceMember } from 'src/actions/tmgr/workspaces';

	interface Props {
		assignees: WorkspaceMember[];
		modelValue: WorkspaceMember['id'];
	}
	const props = defineProps<Props>();
	const assigneeIds = defineModel();
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
						{{
							assigneeIds.length > 0
								? props.assignees
										.filter((assignee) => assigneeIds.includes(assignee.id))
										.map((f) => f.name)
										.join(', ')
								: 'Assignee'
						}}
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
										if (typeof e.detail.value === 'number') {
											if (assigneeIds.includes(e.detail.value)) {
												assigneeIds = assigneeIds.filter(
													(v) => v !== e.detail.value,
												);
											} else {
												assigneeIds.push(e.detail.value);
											}
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
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	</div>
</template>
