<template>
	<Listbox
		as="div"
		v-model="selectedValue"
		@update:model-value="(value) => emit('update:modelValue', value)"
	>
		<div class="relative">
			<ListboxButton
				class="relative w-full cursor-default rounded-md py-1.5 pl-3 pr-10 text-left text-gray-900 sm:text-sm sm:leading-6"
				v-slot="{ open }"
			>
				<span class="flex items-center">
					<slot name="beforeSelectedValue" />

					<span
						class="block truncate dark:text-gray-300"
						:class="[
							!selectedValue?.[label] && placeholder ? 'text-gray-400' : '',
						]"
					>
						{{ selectedValue?.[label] || placeholder }}
					</span>
				</span>

				<span
					class="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2"
				>
					<ChevronDownIcon
						class="h-5 w-5 text-gray-400 transition-transform"
						:class="[open && 'rotate-180']"
						aria-hidden="true"
					/>
				</span>
			</ListboxButton>

			<transition
				leave-active-class="transition ease-in duration-100"
				leave-from-class="opacity-100"
				leave-to-class="opacity-0"
			>
				<ListboxOptions
					class="absolute z-10 mt-1 max-h-60 w-full overflow-auto rounded-md bg-white py-1 text-base shadow-lg dark:bg-gray-800 sm:text-sm"
				>
					<ListboxOption
						as="template"
						v-for="(option, i) in options || []"
						:key="option.id || i"
						:value="option"
						v-slot="{ active, selected }"
					>
						<li
							:class="[
								active
									? 'bg-tmgr-light-blue text-white'
									: 'text-gray-900 dark:text-gray-400',
								'relative cursor-pointer select-none py-2 pl-3 pr-9',
							]"
						>
							<div class="flex items-center">
								<slot name="beforeOption" :option="option" />

								<span
									:class="[
										selected ? 'font-semibold' : 'font-normal',
										'block truncate',
									]"
								>
									{{ option[label] }}
								</span>
							</div>

							<span
								v-if="selected"
								:class="[
									active ? 'text-white' : 'text-tmgr-light-blue',
									'absolute inset-y-0 right-0 flex items-center pr-4',
								]"
							>
								<CheckIcon class="h-5 w-5" aria-hidden="true" />
							</span>
						</li>
					</ListboxOption>
				</ListboxOptions>
			</transition>
		</div>
	</Listbox>
</template>

<script setup lang="ts">
	import { computed, ref } from 'vue';
	import {
		Listbox,
		ListboxButton,
		ListboxLabel,
		ListboxOption,
		ListboxOptions,
	} from '@headlessui/vue';
	import { CheckIcon, ChevronDownIcon } from '@heroicons/vue/20/solid';

	type Option = Record<string, string | number | null | boolean>;

	interface Props {
		label: string;
		options: Option[];
		placeholder: string;
		modelValue: Option | null;
	}

	const props = defineProps<Props>();
	const emit = defineEmits(['update:modelValue']);
	const selectedValue = ref();
</script>
