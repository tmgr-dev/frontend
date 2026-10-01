<template>
	<div class="flex flex-wrap items-center gap-1.5" data-testid="chip-picker">
		<span
			v-for="chip in chips"
			:key="chip.key"
			class="inline-flex items-center gap-1 rounded-full bg-blue-100 py-0.5 pl-2.5 pr-1 text-xs text-blue-800 dark:bg-blue-900/40 dark:text-blue-200"
			data-testid="chip"
		>
			<button
				type="button"
				class="max-w-[14rem] truncate hover:underline"
				@click="$emit('open', chip.key)"
			>
				{{ chip.label }}
			</button>
			<button
				type="button"
				class="rounded-full p-0.5 hover:bg-blue-200 dark:hover:bg-blue-800"
				aria-label="Remove"
				@click="$emit('remove', chip.key)"
			>
				<X class="h-3 w-3" />
			</button>
		</span>

		<div v-if="canAdd" class="relative">
			<button
				v-if="!open"
				type="button"
				class="inline-flex items-center gap-1 rounded-full border border-dashed border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-800"
				@click="show"
			>
				<Plus class="h-3 w-3" />
				{{ addLabel }}
			</button>
			<div v-else class="relative">
				<input
					ref="inputRef"
					v-model="query"
					type="text"
					class="w-48 rounded border border-gray-300 bg-white px-2 py-0.5 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
					:placeholder="placeholder"
					data-testid="chip-picker-input"
					@input="runSearch"
					@blur="close"
					@keydown.esc.prevent="close"
				/>
				<ul
					v-if="items.length"
					class="absolute left-0 top-full z-30 mt-1 max-h-60 w-64 overflow-y-auto rounded-lg border border-gray-200 bg-white py-1 text-sm shadow-lg dark:border-gray-700 dark:bg-gray-800"
					role="listbox"
				>
					<li
						v-for="item in items"
						:key="item.key"
						role="option"
						class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-gray-800 hover:bg-gray-50 dark:text-gray-200 dark:hover:bg-gray-700"
						@mousedown.prevent="choose(item)"
					>
						<span
							v-if="item.hint"
							class="w-14 shrink-0 text-2xs uppercase tracking-wide text-gray-400 dark:text-gray-500"
							>{{ item.hint }}</span
						>
						<span class="truncate">{{ item.label }}</span>
					</li>
				</ul>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { Plus, X } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		type PropType,
		ref,
	} from 'vue';

	export interface ChipOption {
		key: string;
		label: string;
		hint?: string;
	}

	export default defineComponent({
		name: 'ChipPicker',
		components: { Plus, X },
		props: {
			chips: { type: Array as PropType<ChipOption[]>, default: () => [] },
			search: {
				type: Function as PropType<(query: string) => Promise<ChipOption[]>>,
				required: true,
			},
			single: { type: Boolean, default: false },
			addLabel: { type: String, default: 'Add' },
			placeholder: { type: String, default: 'Search' },
		},
		emits: ['add', 'remove', 'open'],
		setup(props, { emit }) {
			const open = ref(false);
			const query = ref('');
			const items = ref<ChipOption[]>([]);
			const inputRef = ref<HTMLInputElement | null>(null);
			let seq = 0;
			let timer: ReturnType<typeof setTimeout> | undefined;

			const canAdd = computed(() => !props.single || !props.chips.length);

			const load = async () => {
				const current = ++seq;
				const found = await props.search(query.value.trim());
				if (current !== seq) return;
				const taken = new Set(props.chips.map((chip) => chip.key));
				items.value = found.filter((item) => !taken.has(item.key));
			};

			const runSearch = () => {
				clearTimeout(timer);
				timer = setTimeout(load, 150);
			};

			const show = async () => {
				open.value = true;
				query.value = '';
				await nextTick();
				inputRef.value?.focus();
				void load();
			};

			const close = () => {
				clearTimeout(timer);
				seq++;
				open.value = false;
				items.value = [];
			};

			const choose = (item: ChipOption) => {
				emit('add', item.key);
				close();
			};

			onBeforeUnmount(() => clearTimeout(timer));

			return {
				open,
				query,
				items,
				inputRef,
				canAdd,
				runSearch,
				show,
				close,
				choose,
			};
		},
	});
</script>
