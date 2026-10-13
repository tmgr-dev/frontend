import type { RunnerState, TermFrame } from '@/runner/api';
import { describeState, sanitizeState, type StateView } from '@/runner/states';

export interface TermLike {
	write(data: string, done?: () => void): void;
	rows: number;
}

export interface TermHostApi {
	ack(seq: number): unknown;
	input(data: string): unknown;
	scroll(offset: number): unknown;
	detach(): unknown;
	openLink(url: string): unknown;
}

export interface HeaderState extends StateView {
	detail: string | null;
}

export interface TermView {
	setHeader(header: HeaderState): void;
	setScrollText(text: string | null): void;
	showClosed(reason: string): void;
	hideClosed(): void;
	closeWindow(): void;
}

const WHEEL_STEP = 3;

export const scrollIndicatorText = (offset: number): string =>
	`Scrolled back ${offset} ${
		offset === 1 ? 'line' : 'lines'
	} — press End to return`;

export const createController = (deps: {
	term: TermLike;
	api: TermHostApi;
	view: TermView;
}) => {
	const { term, api, view } = deps;
	let offset = 0;
	let max = 0;
	let closed = false;

	const scrollTo = (target: number) => {
		const next = Math.max(0, Math.min(max, target));
		if (next === offset) return;
		void api.scroll(next);
	};

	return {
		handleFrame(frame: TermFrame) {
			switch (frame.type) {
				case 'data':
					term.write(frame.data, () => void api.ack(frame.seq));
					break;
				case 'state':
					view.setHeader({
						...describeState(sanitizeState(frame.state as RunnerState)),
						detail: frame.state.detail,
					});
					break;
				case 'scroll':
					offset = frame.offset;
					max = frame.max;
					view.setScrollText(offset > 0 ? scrollIndicatorText(offset) : null);
					break;
				case 'closed':
					closed = true;
					view.showClosed(frame.reason);
					break;
			}
		},
		onInput(data: string) {
			if (!closed) void api.input(data);
		},
		wheel(deltaY: number) {
			if (deltaY === 0) return;
			scrollTo(offset + (deltaY < 0 ? WHEEL_STEP : -WHEEL_STEP));
		},
		handleKey(event: { key: string; shiftKey: boolean }): boolean {
			const page = Math.max(1, term.rows - 1);
			if (event.shiftKey && event.key === 'PageUp') {
				scrollTo(offset + page);
				return true;
			}
			if (event.shiftKey && event.key === 'PageDown') {
				scrollTo(offset - page);
				return true;
			}
			if (event.key === 'End' && !event.shiftKey && offset > 0) {
				scrollTo(0);
				return true;
			}
			return false;
		},
		openLink(url: string) {
			void api.openLink(url);
		},
		async detach() {
			await api.detach();
			view.closeWindow();
		},
		reopened() {
			closed = false;
			offset = 0;
			max = 0;
			view.hideClosed();
			view.setScrollText(null);
		},
	};
};
