<template>
	<div ref="root" class="tour-menu">
		<button
			ref="trigger"
			type="button"
			class="tour-btn"
			aria-haspopup="menu"
			:aria-expanded="open ? 'true' : 'false'"
			:aria-controls="menuId"
			@click="toggle"
			@keydown.down.prevent="openAndFocus(0)"
		>
			{{ label }}
			<svg
				width="12"
				height="12"
				viewBox="0 0 12 12"
				aria-hidden="true"
				fill="none"
				stroke="currentColor"
				stroke-width="1.6"
			>
				<path d="M2.5 4.5 6 8l3.5-3.5" />
			</svg>
		</button>
		<ul
			v-if="open"
			:id="menuId"
			role="menu"
			class="tour-menu__list"
			:data-side="side"
			:data-align="align"
			data-state="open"
			data-tour-menu="open"
			:aria-label="label"
			@keydown="onKeydown"
		>
			<li v-for="item in visibleItems" :key="item.key" role="none">
				<button
					type="button"
					role="menuitem"
					class="tour-menu__item"
					tabindex="-1"
					@click="choose(item.key)"
				>
					<span class="tour-menu__label">{{ item.label }}</span>
					<span class="tour-menu__hint">{{ item.hint }}</span>
				</button>
			</li>
		</ul>
	</div>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		ref,
		type PropType,
	} from 'vue';

	export type SkipMenuKey =
		'skip-step' | 'pause' | 'explore' | 'never' | 'skip-tour';

	const ITEM_KEYS: Array<{ key: SkipMenuKey; path: string }> = [
		{ key: 'skip-step', path: 'onboarding.tours.skipMenu.skipStep' },
		{ key: 'pause', path: 'onboarding.tours.skipMenu.pause' },
		{ key: 'explore', path: 'onboarding.tours.skipMenu.explore' },
		{ key: 'never', path: 'onboarding.tours.skipMenu.never' },
		{ key: 'skip-tour', path: 'onboarding.tours.skipMenu.skipTour' },
	];

	let counter = 0;

	export default defineComponent({
		name: 'TourSkipMenu',
		props: {
			keys: {
				type: Array as PropType<SkipMenuKey[]>,
				default: () => ['skip-step', 'pause', 'explore', 'never', 'skip-tour'],
			},
			label: { type: String, default: () => t('onboarding.tours.ui.skip') },
			align: { type: String as PropType<'left' | 'right'>, default: 'left' },
		},
		emits: ['select'],
		setup(props, { emit }) {
			counter += 1;
			const menuId = `tour-menu-${counter}`;
			const open = ref(false);
			const side = ref<'above' | 'below'>('below');
			const root = ref<HTMLElement | null>(null);
			const trigger = ref<HTMLButtonElement | null>(null);

			const visibleItems = computed(() =>
				ITEM_KEYS.filter((i) => props.keys.includes(i.key)).map((i) => ({
					key: i.key,
					label: t(`${i.path}.label`),
					hint: t(`${i.path}.hint`),
				})),
			);

			const items = () =>
				Array.from(
					root.value?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [],
				);

			const onOutside = (e: Event) => {
				if (root.value && !root.value.contains(e.target as Node)) close(false);
			};

			function close(refocus: boolean) {
				open.value = false;
				document.removeEventListener('pointerdown', onOutside, true);
				if (refocus) trigger.value?.focus();
			}

			async function openAndFocus(index: number) {
				if (!open.value) {
					const rect = trigger.value?.getBoundingClientRect();
					side.value =
						rect && window.innerHeight - rect.bottom < 280 ? 'above' : 'below';
					open.value = true;
					document.addEventListener('pointerdown', onOutside, true);
				}
				await nextTick();
				items()[index]?.focus();
			}

			function toggle() {
				if (open.value) close(false);
				else void openAndFocus(0);
			}

			function choose(key: SkipMenuKey) {
				close(false);
				emit('select', key);
			}

			function onKeydown(e: KeyboardEvent) {
				const list = items();
				const index = list.indexOf(document.activeElement as HTMLElement);
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					list[(index + 1) % list.length]?.focus();
				} else if (e.key === 'ArrowUp') {
					e.preventDefault();
					list[(index - 1 + list.length) % list.length]?.focus();
				} else if (e.key === 'Home') {
					e.preventDefault();
					list[0]?.focus();
				} else if (e.key === 'End') {
					e.preventDefault();
					list[list.length - 1]?.focus();
				} else if (e.key === 'Escape') {
					e.preventDefault();
					e.stopPropagation();
					close(true);
				} else if (e.key === 'Tab') {
					close(false);
				}
			}

			onBeforeUnmount(() =>
				document.removeEventListener('pointerdown', onOutside, true),
			);

			return {
				menuId,
				open,
				side,
				root,
				trigger,
				visibleItems,
				toggle,
				openAndFocus,
				choose,
				onKeydown,
			};
		},
	});
</script>
