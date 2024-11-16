<script setup lang="ts">
	import { cn } from '@/utils';
	import {
		Command,
		CommandEmpty,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from '@/components/ui/command';
	import { FolderIcon } from '@heroicons/vue/24/outline';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { Button } from '@/components/ui/button';
	import { ref, computed } from 'vue';
	import { Category } from '@/actions/tmgr/categories';

	interface Props {
		categories: Category[];
		modelValue: Category['id'];
		class?: string;
	}

	const props = defineProps<Props>();
	const categoryId = defineModel();
	const openCombobox = ref(false);
	const searchValue = ref('');
	const filteredCategories = computed(() => {
		if (!searchValue.value) return props.categories;

		return props.categories.filter((category) =>
			category.title.toLowerCase().includes(searchValue.value.toLowerCase()),
		);
	});
</script>

<template>
	<div class="flex items-center gap-2">
		<FolderIcon class="size-5 shrink-0" />

		<Popover v-model:open="openCombobox">
			<PopoverTrigger as-child>
				<Button
					variant="ghost"
					role="combobox"
					:aria-expanded="openCombobox"
					class="w-32 justify-between overflow-hidden px-0"
					:class="props.class"
				>
					<span class="truncate">
						{{
							categoryId
								? props.categories.find(
										(category) => category.id === categoryId,
								  )?.title
								: 'Category'
						}}
					</span>
					<ChevronsUpDown class="ml-1 h-4 w-4 shrink-0 opacity-50" />
				</Button>
			</PopoverTrigger>

			<PopoverContent
				class="w-52 rounded bg-white p-0 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400"
				:class="props.class"
			>
				<Command>
					<CommandInput
						class="h-9"
						wrapper-class="dark:border-gray-600"
						placeholder="Search category..."
						@input="(e) => (searchValue = e.target.value)"
					/>
					<CommandEmpty>No category found.</CommandEmpty>
					<CommandList class="w-80">
						<CommandGroup>
							<CommandItem
								v-for="category in filteredCategories"
								:key="category.slug"
								:value="category.id"
								@select="
									(e) => {
										if (e.detail.value) {
											categoryId = e.detail.value;
											searchValue = '';
										}
										openCombobox = false;
									}
								"
								class="cursor-pointer text-gray-900 hover:!bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
							>
								{{ category.title }}
								<Check
									:class="
										cn(
											'ml-auto h-4 w-4',
											categoryId === category.id ? 'opacity-100' : 'opacity-0',
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
