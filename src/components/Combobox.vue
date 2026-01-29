<script setup lang="ts">
	import { cn } from '@/utils';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import { Button } from '@/components/ui/button';
	import {
		Command,
		CommandEmpty,
		CommandGroup,
		CommandInput,
		CommandItem,
		CommandList,
	} from '@/components/ui/command';
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import { computed, ref } from 'vue';

	interface Props {
		entities: any[];
		labelKey: string;
		valueKey: string;
		selectedPlaceholder: string;
		inputPlaceholder?: string;
		notFoundMessage?: string;
	}

	const props = defineProps<Props>();
	const modelValue = defineModel<string | number | null>();
	const emit = defineEmits(['update:modelValue']);
	const openCombobox = ref(false);
	const searchValue = ref('');
	const filteredEntities = computed(() => {
		if (!searchValue.value) return props.entities;

		return props.entities.filter((entity) =>
			entity[props.labelKey]
				.toLowerCase()
				.includes(searchValue.value.toLowerCase()),
		);
	});

	const handleSelect = (e: { detail: { value: string | number | null } }) => {
		if (e.detail.value) {
			modelValue.value = e.detail.value;
			searchValue.value = '';
		}
		openCombobox.value = false;
	};
</script>

<template>
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
							? entities.find((entity) => entity[valueKey] == modelValue)?.[
									labelKey
							  ]
							: selectedPlaceholder
					}}
				</span>
				<ChevronsUpDown class="ml-1 h-4 w-4 shrink-0 opacity-50" />
			</Button>
		</PopoverTrigger>

		<PopoverContent
			class="rounded bg-white p-0 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400"
		>
			<Command>
				<CommandInput
					class="h-9"
					wrapper-class="dark:border-gray-600"
					:placeholder="inputPlaceholder || 'Search entities'"
					@input="(e: InputEvent) => (searchValue = (e.target as HTMLInputElement).value)"
				/>
				<CommandEmpty>{{ notFoundMessage || 'No entity found.' }}</CommandEmpty>
				<CommandList class="w-80">
					<CommandGroup>
						<CommandItem
							v-for="entity in filteredEntities"
							:key="entity.id"
							:value="entity[valueKey]"
							@select="handleSelect"
							class="cursor-pointer text-gray-900 hover:!bg-tmgr-light-blue hover:!text-white dark:text-gray-400"
						>
							{{ entity[labelKey] }}
							<Check
								:class="
									cn(
										'ml-auto h-4 w-4',
										modelValue === entity[valueKey]
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
</template>
