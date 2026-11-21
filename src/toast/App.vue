<template>
	<div ref="root" class="toast-root">
		<div class="stackbox" role="region" :aria-label="$t('toast.window.label')">
			<div
				v-for="item in list.visible"
				:key="item.id"
				class="toast"
				role="alert"
				@pointerenter="setHover(item.id, 'hover', true)"
				@pointerleave="setHover(item.id, 'hover', false)"
				@focusin="setHover(item.id, 'focus', true)"
				@focusout="setHover(item.id, 'focus', false)"
			>
				<button
					type="button"
					class="tx"
					:aria-label="$t('toast.dismiss')"
					@click="dismiss(item.id)"
				>
					<svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
						<path
							d="M2 2l8 8M10 2l-8 8"
							stroke="currentColor"
							stroke-width="1.6"
							stroke-linecap="round"
							fill="none"
						/>
					</svg>
				</button>
				<div class="ti" :class="{ ok: isOk(item.kind) }">
					{{ glyphOf(item.kind) }}
				</div>
				<div class="th">{{ item.title }}</div>
				<div class="tt">{{ ageLabel(clock, item.at) }}</div>
				<div class="tb">
					{{ item.body }}
					<span v-if="item.meta" class="m">{{ item.meta }}</span>
				</div>
				<div v-if="item.actions.length" class="ta">
					<button
						v-for="action in item.actions"
						:key="action.id"
						type="button"
						:class="{ rec: action.primary }"
						@click="act(item.id, action.id)"
					>
						{{ action.label }}
					</button>
				</div>
				<div
					v-if="drainStyle(item)"
					:key="`${item.id}:${item.remainingMs}:${item.paused}`"
					class="prog"
					:style="drainStyle(item) ?? undefined"
				></div>
			</div>
			<button
				v-if="list.more > 0 && !list.expanded"
				type="button"
				class="toast stack"
				@click="expand"
			>
				<span class="ti small">+{{ list.more }}</span>
				<span class="th muted">{{
					$t('toast.more', { count: list.more })
				}}</span>
			</button>
			<div v-else-if="list.more > 0" class="toast stack static">
				<span class="ti small">+{{ list.more }}</span>
				<span class="th muted">{{
					$t('toast.moreHidden', { count: list.more })
				}}</span>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, onBeforeUnmount, onMounted, ref } from 'vue';
	import {
		onToastList,
		toastAction,
		toastDismiss,
		toastExpand,
		toastHover,
		toastResize,
		toastState,
	} from './api';
	import {
		ageLabel,
		drainStyle,
		glyphOf,
		heightOf,
		hoverKey,
		isOk,
	} from './model';
	import type { ToastList } from './types';

	export default defineComponent({
		name: 'ToastApp',
		setup() {
			const root = ref<HTMLElement | null>(null);
			const list = ref<ToastList>({ visible: [], more: 0, expanded: false });
			const clock = ref(Date.now());
			let unlisten: (() => void) | null = null;
			let observer: ResizeObserver | null = null;
			let timer: ReturnType<typeof setInterval> | null = null;
			let lastHeight = 0;

			const report = () => {
				const height = heightOf(root.value?.offsetHeight ?? 0);
				if (!height || height === lastHeight) return;
				lastHeight = height;
				void toastResize(height).catch(() => undefined);
			};

			const act = (toastId: string, actionId: string) => {
				void toastAction(toastId, actionId).catch((error) =>
					console.error('toast action failed', error),
				);
			};

			const dismiss = (toastId: string) => {
				void toastDismiss(toastId).catch(() => undefined);
			};

			const pointer = new Map<string, { hover: boolean; focus: boolean }>();
			const setHover = (
				toastId: string,
				part: 'hover' | 'focus',
				value: boolean,
			) => {
				const prev = pointer.get(toastId) ?? { hover: false, focus: false };
				const next = { ...prev, [part]: value };
				pointer.set(toastId, next);
				if (
					hoverKey(prev.hover, prev.focus) !== hoverKey(next.hover, next.focus)
				) {
					void toastHover(toastId, hoverKey(next.hover, next.focus)).catch(
						() => undefined,
					);
				}
			};

			const expand = () => {
				void toastExpand().catch(() => undefined);
			};

			onMounted(async () => {
				if (root.value && typeof ResizeObserver !== 'undefined') {
					observer = new ResizeObserver(report);
					observer.observe(root.value);
				}
				timer = setInterval(() => (clock.value = Date.now()), 30_000);
				unlisten = await onToastList((next) => {
					list.value = next;
					clock.value = Date.now();
				});
				try {
					list.value = await toastState();
				} catch (error) {
					console.error('toast state failed', error);
				}
				report();
			});

			onBeforeUnmount(() => {
				unlisten?.();
				observer?.disconnect();
				if (timer) clearInterval(timer);
			});

			return {
				root,
				list,
				clock,
				act,
				dismiss,
				setHover,
				expand,
				ageLabel,
				drainStyle,
				glyphOf,
				isOk,
			};
		},
	});
</script>
