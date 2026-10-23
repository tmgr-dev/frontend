<script setup lang="ts">
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
	import { Check, ChevronsUpDown } from 'lucide-vue-next';
	import type { ComboboxItemEmits } from 'radix-vue';
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

	const handleSelect = (e: ComboboxItemEmits['select'][0]) => {
		if (e.detail.value) {
			// entity[valueKey] values are strings or numbers; radix types them as AcceptableValue
			modelValue.value = e.detail.value as string | number;
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

		<PopoverContent class="rounded p-0">
			<Command>
				<CommandInput
					class="h-9"
					:placeholder="inputPlaceholder || 'Search entities'"
					@input="(e: InputEvent) => (searchValue = (e.target as HTMLInputElement).value)"
				/>
				<CommandEmpty class="text-ink-muted">{{
					notFoundMessage || 'No entity found.'
				}}</CommandEmpty>
				<CommandList class="w-80">
					<CommandGroup>
						<CommandItem
							v-for="entity in filteredEntities"
							:key="entity.id"
							:value="entity[valueKey]"
							@select="handleSelect"
							class="cursor-pointer"
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
