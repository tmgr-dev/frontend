import { onLocaleChange, t } from '@/i18n';
import {
	runnerTermAck,
	runnerTermAttach,
	runnerTermDetach,
	runnerTermInput,
	runnerTermOpenLink,
	runnerTermResize,
	runnerTermScroll,
	type RunnerState,
} from '@/runner/api';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { createController, type HeaderState } from './controller';
import './terminal.css';

const LIGHT = {
	background: '#ffffff',
	foreground: '#1b1f24',
	cursor: '#1b1f24',
	selectionBackground: '#b6d4fe',
};
const DARK = {
	background: '#14171b',
	foreground: '#e6e9ed',
	cursor: '#e6e9ed',
	selectionBackground: '#2f4a77',
};

const el = <K extends keyof HTMLElementTagNameMap>(
	tag: K,
	className?: string,
	text?: string,
): HTMLElementTagNameMap[K] => {
	const node = document.createElement(tag);
	if (className) node.className = className;
	if (text !== undefined) node.textContent = text;
	return node;
};

export const start = () => {
	const root = document.getElementById('root') as HTMLElement;
	const bar = el('div', 'bar');
	const name = el('span', 'name');
	const chip = el('span', 'chip');
	const detail = el('span', 'detail');
	const scrollNote = el('span', 'scroll');
	scrollNote.hidden = true;
	const notice = el('span', 'notice');
	notice.hidden = true;
	notice.setAttribute('role', 'alert');
	const spacer = el('span', 'spacer');
	const detachButton = el('button');
	detachButton.type = 'button';
	bar.append(name, chip, detail, scrollNote, notice, spacer, detachButton);

	const host = el('div');
	host.id = 'term';

	const overlay = el('div', 'overlay');
	overlay.hidden = true;
	const overlayText = el('div');
	const reattachButton = el('button');
	reattachButton.type = 'button';
	overlay.append(overlayText, reattachButton);

	root.append(bar, host, overlay);

	let sessionName: string | null = null;
	let closedReason: string | null = null;
	let lastState: RunnerState | null = null;
	let hasHeader = false;

	const renderLabels = () => {
		name.textContent = sessionName ?? t('runnerTerminal.agentSession');
		document.title = sessionName ?? t('runnerTerminal.title');
		detachButton.textContent = t('runnerTerminal.detach');
		reattachButton.textContent = t('runnerTerminal.reattach');
		if (!hasHeader) chip.textContent = t('runnerTerminal.connecting');
		if (closedReason !== null) {
			overlayText.textContent = t('runnerTerminal.sessionEnded', {
				reason: closedReason,
			});
		}
	};

	const dark = window.matchMedia('(prefers-color-scheme: dark)');

	const term = new Terminal({
		scrollback: 0,
		convertEol: false,
		allowProposedApi: false,
		cursorBlink: true,
		fontSize: 13,
		fontFamily: 'Menlo, Monaco, "Courier New", monospace',
		theme: dark.matches ? DARK : LIGHT,
	});
	const fit = new FitAddon();
	term.loadAddon(fit);
	term.loadAddon(
		new WebLinksAddon((_event, uri) => {
			controller.openLink(uri);
		}),
	);
	term.parser.registerOscHandler(52, () => true);
	dark.addEventListener('change', (event) => {
		term.options.theme = event.matches ? DARK : LIGHT;
	});

	const controller = createController({
		term,
		api: {
			ack: runnerTermAck,
			input: runnerTermInput,
			scroll: runnerTermScroll,
			detach: runnerTermDetach,
			openLink: runnerTermOpenLink,
		},
		view: {
			setHeader(header: HeaderState) {
				hasHeader = true;
				chip.textContent = header.label;
				chip.className = `chip ${header.tone}`;
				bar.classList.toggle('attention', header.highlight);
				detail.textContent = header.detail ?? '';
			},
			setScrollText(text) {
				scrollNote.hidden = text === null;
				scrollNote.textContent = text ?? '';
			},
			setNotice(text) {
				notice.hidden = text === null;
				notice.textContent = text ?? '';
			},
			showClosed(reason) {
				closedReason = reason;
				overlayText.textContent = t('runnerTerminal.sessionEnded', { reason });
				overlay.hidden = false;
			},
			hideClosed() {
				closedReason = null;
				overlay.hidden = true;
			},
			closeWindow() {
				window.close();
			},
		},
	});

	term.attachCustomKeyEventHandler(
		(event) => !(event.type === 'keydown' && controller.handleKey(event)),
	);
	host.addEventListener(
		'wheel',
		(event) => {
			event.preventDefault();
			event.stopPropagation();
			controller.wheel(event.deltaY);
		},
		{ capture: true, passive: false },
	);
	term.onData((data) => controller.onInput(data));
	term.onResize(({ cols, rows }) => void runnerTermResize(cols, rows));

	const attach = async () => {
		const info = await runnerTermAttach(
			(frame) => {
				if (frame.type === 'state') lastState = frame.state as RunnerState;
				controller.handleFrame(frame);
			},
			term.cols,
			term.rows,
		);
		sessionName = info.name;
		renderLabels();
		const state = lastState ?? (info.state as RunnerState);
		lastState = state;
		controller.handleFrame({ type: 'state', state });
	};

	detachButton.addEventListener('click', () => void controller.detach());
	reattachButton.addEventListener('click', () => {
		controller.reopened();
		term.reset();
		void attach().catch((error) =>
			controller.handleFrame({ type: 'closed', reason: String(error) }),
		);
	});

	renderLabels();
	onLocaleChange(() => {
		renderLabels();
		if (lastState) controller.handleFrame({ type: 'state', state: lastState });
		controller.relocalize();
	});

	term.open(host);
	fit.fit();
	new ResizeObserver(() => fit.fit()).observe(host);
	term.focus();
	attach().catch((error) =>
		controller.handleFrame({ type: 'closed', reason: String(error) }),
	);
};
