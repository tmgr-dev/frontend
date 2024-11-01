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
	import { FolderIcon } from '@heroicons/vue/24/outline';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from 'src/components/ui/popover';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { Button } from 'src/components/ui/button';
	import { ref, defineProps, defineEmits, watch } from 'vue';
	import { Category } from 'src/actions/tmgr/categories';

	interface Props {
		categories: Category[];
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['update']);

	const openCombobox = ref(false);
	const selectedCategory = ref<Category['title']>();
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
				>
					<span class="truncate">
						{{
							selectedCategory
								? props.categories.find(
										(category) => category.title === selectedCategory,
								  )?.title
								: 'Category'
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
						placeholder="Search category..."
					/>
					<CommandEmpty>No category found.</CommandEmpty>
					<CommandList>
						<CommandGroup>
							<CommandItem
								v-for="category in props.categories"
								:key="category.slug"
								:value="category.title"
								@select="
									(e) => {
										if (e.detail.value) {
											selectedCategory = e.detail.value;
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
											selectedCategory === category.title
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
