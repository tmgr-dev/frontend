<template>
	<div class="flex flex-wrap items-center gap-1.5" data-testid="chip-picker">
		<span
			v-for="chip in chips"
			:key="chip.key"
			class="inline-flex items-center gap-1 rounded-full bg-tag-bg py-0.5 pr-1 pl-2.5 text-xs text-tag-fg"
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
				class="rounded-full p-0.5 hover:text-ink"
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
				class="inline-flex items-center gap-1 rounded-full border border-dashed border-line-strong px-2 py-0.5 text-xs text-ink-muted hover:bg-surface-hover"
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
					class="w-48 rounded border border-input bg-background px-2 py-0.5 text-sm text-ink"
					:placeholder="placeholder"
					data-testid="chip-picker-input"
					@input="runSearch"
					@blur="close"
					@keydown.esc.prevent="close"
				/>
				<ul
					v-if="items.length"
					class="absolute top-full left-0 z-30 mt-1 max-h-60 w-64 overflow-y-auto rounded-lg border border-line bg-popover py-1 text-sm text-popover-foreground shadow-lg"
					role="listbox"
				>
					<li
						v-for="item in items"
						:key="item.key"
						role="option"
						class="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-ink hover:bg-surface-hover"
						@mousedown.prevent="choose(item)"
					>
						<span
							v-if="item.hint"
							class="w-14 shrink-0 text-2xs tracking-wide text-ink-faint uppercase"
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
