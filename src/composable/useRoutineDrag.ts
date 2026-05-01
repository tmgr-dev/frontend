import { onBeforeUnmount, ref } from 'vue';
import type { RoutineEntry } from '@/types/dailyRoutine';

const LONG_PRESS_MS = 250;
const TOUCH_HOLD_MS = 350;
const MOVE_THRESHOLD = 5;

export type DropPayload = {
	date: string;
	timeH?: number;
	timeM?: number;
	allDay?: boolean;
};

type DragState = {
	entry: RoutineEntry;
	pointerId: number;
	pointerType: string;
	x: number;
	y: number;
	ghostEl: HTMLElement | null;
};

const active = ref<RoutineEntry | null>(null);
const hoverKey = ref<string | null>(null);

let state: DragState | null = null;
let pressTimer: number | null = null;
let dropHandler: ((entry: RoutineEntry, payload: DropPayload) => void) | null = null;
let editHandler: ((entry: RoutineEntry) => void) | null = null;
let canceled = false;

function fmtIso(d: Date): string {
	const y = d.getFullYear();
	const m = String(d.getMonth() + 1).padStart(2, '0');
	const day = String(d.getDate()).padStart(2, '0');
	return `${y}-${m}-${day}`;
}

function clearTimer() {
	if (pressTimer !== null) {
		window.clearTimeout(pressTimer);
		pressTimer = null;
	}
}

function buildGhost(srcEl: HTMLElement): HTMLElement {
	const rect = srcEl.getBoundingClientRect();
	const ghost = srcEl.cloneNode(true) as HTMLElement;
	ghost.style.position = 'fixed';
	ghost.style.left = `${rect.left}px`;
	ghost.style.top = `${rect.top}px`;
	ghost.style.width = `${rect.width}px`;
	ghost.style.height = `${rect.height}px`;
	ghost.style.pointerEvents = 'none';
	ghost.style.zIndex = '99999';
	ghost.style.opacity = '0.85';
	ghost.style.transform = 'scale(1.02)';
	ghost.style.boxShadow = '0 12px 32px rgba(0,0,0,0.35)';
	ghost.style.transition = 'transform 80ms ease';
	document.body.appendChild(ghost);
	return ghost;
}

function moveGhost(x: number, y: number) {
	if (!state?.ghostEl) return;
	const rect = state.ghostEl.getBoundingClientRect();
	state.ghostEl.style.left = `${x - rect.width / 2}px`;
	state.ghostEl.style.top = `${y - 16}px`;
}

function pickDropTarget(x: number, y: number): { el: HTMLElement; payload: DropPayload } | null {
	const els = document.elementsFromPoint(x, y);
	for (const el of els) {
		const target = (el as HTMLElement).closest('[data-dr-drop]') as HTMLElement | null;
		if (!target) continue;
		const date = target.dataset.drDate;
		if (!date) continue;
		const kind = target.dataset.drKind;
		if (kind === 'unscheduled' || kind === 'all-day') {
			return { el: target, payload: { date, allDay: true } };
		}
		if (kind === 'hour-grid') {
			const rect = target.getBoundingClientRect();
			const ratio = Math.max(0, Math.min(0.999, (y - rect.top) / rect.height));
			const totalMin = Math.round((ratio * 24 * 60) / 30) * 30;
			const h = Math.min(23, Math.floor(totalMin / 60));
			const m = totalMin % 60;
			return { el: target, payload: { date, timeH: h, timeM: m } };
		}
		if (kind === 'day-cell') {
			return { el: target, payload: { date } };
		}
	}
	return null;
}

function onPointerMove(e: PointerEvent) {
	if (!state) return;
	if (state.pointerId !== e.pointerId) return;
	const dx = e.clientX - state.x;
	const dy = e.clientY - state.y;
	if (!active.value) {
		if (Math.hypot(dx, dy) > MOVE_THRESHOLD) {
			cancel();
		}
		return;
	}
	e.preventDefault();
	moveGhost(e.clientX, e.clientY);
	const target = pickDropTarget(e.clientX, e.clientY);
	hoverKey.value = target ? targetKey(target.payload) : null;
}

function targetKey(p: DropPayload): string {
	if (p.allDay) return `unsched:${p.date}`;
	if (p.timeH != null) return `slot:${p.date}:${p.timeH}:${p.timeM}`;
	return `day:${p.date}`;
}

function onPointerUp(e: PointerEvent) {
	if (!state || state.pointerId !== e.pointerId) return;
	const wasActive = !!active.value;
	const drop = wasActive ? pickDropTarget(e.clientX, e.clientY) : null;
	cleanup();
	if (drop && wasActive && dropHandler) {
		const entry = active.value;
		if (entry) dropHandler(entry, drop.payload);
	}
	active.value = null;
	hoverKey.value = null;
}

function onPointerCancel() {
	cancel();
}

function cancel() {
	cleanup();
	active.value = null;
	hoverKey.value = null;
}

function cleanup() {
	clearTimer();
	if (state?.ghostEl) {
		state.ghostEl.remove();
	}
	if (state) {
		try {
			document.removeEventListener('pointermove', onPointerMove);
			document.removeEventListener('pointerup', onPointerUp);
			document.removeEventListener('pointercancel', onPointerCancel);
		} catch {}
	}
	document.body.style.userSelect = '';
	state = null;
	canceled = false;
}

export function useRoutineDrag() {
	function setDropHandler(fn: (entry: RoutineEntry, payload: DropPayload) => void) {
		dropHandler = fn;
	}

	function setEditHandler(fn: (entry: RoutineEntry) => void) {
		editHandler = fn;
	}

	function onPointerDown(e: PointerEvent, entry: RoutineEntry, srcEl?: HTMLElement) {
		if (e.button != null && e.button !== 0) return;
		if (active.value) return;
		const target = e.target as HTMLElement | null;
		if (target && target.closest('button, a, input, textarea, select, [data-no-drag]')) {
			return;
		}
		const el = (srcEl ?? (e.currentTarget as HTMLElement)) || null;
		if (!el) return;
		const pointerType = e.pointerType || 'mouse';
		state = {
			entry,
			pointerId: e.pointerId,
			pointerType,
			x: e.clientX,
			y: e.clientY,
			ghostEl: null,
		};
		canceled = false;
		document.addEventListener('pointermove', onPointerMove, { passive: false });
		document.addEventListener('pointerup', onPointerUp);
		document.addEventListener('pointercancel', onPointerCancel);
		const isTouch = pointerType === 'touch';
		const delay = isTouch ? TOUCH_HOLD_MS : LONG_PRESS_MS;
		pressTimer = window.setTimeout(() => {
			if (!state) return;
			if (canceled) return;
			if (isTouch) {
				// touch hold → open edit modal, no drag
				const ent = state.entry;
				cleanup();
				active.value = null;
				hoverKey.value = null;
				if (editHandler) editHandler(ent);
				return;
			}
			active.value = entry;
			document.body.style.userSelect = 'none';
			state.ghostEl = buildGhost(el);
			if (navigator.vibrate) {
				try { navigator.vibrate(15); } catch {}
			}
			moveGhost(state.x, state.y);
		}, delay);
	}

	onBeforeUnmount(() => {
		if (state) cancel();
	});

	return {
		active,
		hoverKey,
		onPointerDown,
		setDropHandler,
		setEditHandler,
		fmtIso,
	};
}

export const routineDragKey = (p: DropPayload) => targetKey(p);
