import { compileScript, compileTemplate, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';
import { proxyRefs, shallowReactive, type VNode } from 'vue';

const mockWriteClipboard = jest.fn();

jest.mock('@/utils/share', () => ({
	writeClipboard: (...args: unknown[]) => mockWriteClipboard(...args),
}));
jest.mock('@/components/agent/AgentConfirmationCard.vue', () => ({
	__esModule: true,
	default: 'Card',
}));
jest.mock('@/components/attachments/AttachedFileList.vue', () => ({
	__esModule: true,
	default: 'Files',
}));
jest.mock('@/components/general/MarkdownText.vue', () => ({
	__esModule: true,
	default: 'Markdown',
}));
jest.mock('vue', () => ({
	...jest.requireActual('vue'),
	onBeforeUnmount: jest.fn(),
}));
jest.mock('lucide-vue-next', () => ({
	Check: 'Check',
	Copy: 'Copy',
	Loader2: 'Loader2',
	Pin: 'Pin',
	Reply: 'Reply',
}));

const toCommonJs = (code: string): string =>
	ts.transpileModule(code, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
		},
	}).outputText;

const run = (code: string): Record<string, any> => {
	const module = { exports: {} as Record<string, any> };
	new Function('require', 'module', 'exports', toCommonJs(code))(
		require,
		module,
		module.exports,
	);
	return module.exports;
};

const loadComponent = () => {
	const filename = join(__dirname, '../AgentMessage.vue');
	const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });
	const script = compileScript(descriptor, { id: 'agent-message' });
	const template = compileTemplate({
		source: descriptor.template!.content,
		filename,
		id: 'agent-message',
		compilerOptions: { bindingMetadata: script.bindings },
	});
	return {
		setup: run(script.content).default.setup as (
			props: object,
			ctx: object,
		) => Record<string, any>,
		render: run(template.code).render as (...args: unknown[]) => VNode,
	};
};

const { setup, render } = loadComponent();

type Message = Record<string, unknown>;

const mount = (message: Message, extra: Record<string, unknown> = {}) => {
	const props = shallowReactive({ message, ...extra });
	const emitted: [string, ...unknown[]][] = [];
	const bindings = proxyRefs(
		setup(props, {
			emit: (name: string, ...args: unknown[]) => emitted.push([name, ...args]),
			expose: () => undefined,
			attrs: {},
			slots: {},
		}),
	);
	const tree = () => render({}, [], props, bindings, {}, {});
	return { tree, emitted, bindings };
};

const walk = (node: unknown, visit: (vnode: VNode) => void): void => {
	if (Array.isArray(node)) return node.forEach((n) => walk(n, visit));
	if (!node || typeof node !== 'object') return;
	const vnode = node as VNode;
	visit(vnode);
	if (typeof vnode.type === 'string' || typeof vnode.type === 'symbol')
		walk(vnode.children, visit);
};

const text = (node: unknown): string => {
	let out = '';
	const collect = (n: unknown): void => {
		if (typeof n === 'string') out += `${n} `;
		else if (Array.isArray(n)) n.forEach(collect);
		else if (n && typeof n === 'object') {
			const v = n as VNode;
			if (typeof v.type === 'string' || typeof v.type === 'symbol')
				collect(v.children);
		}
	};
	collect(node);
	return out.replace(/\s+/g, ' ').trim();
};

const find = (tree: VNode, pick: (v: VNode) => boolean): VNode | undefined => {
	let found: VNode | undefined;
	walk(tree, (v) => {
		if (!found && pick(v)) found = v;
	});
	return found;
};

const button = (tree: VNode, label: string) =>
	find(tree, (v) => v.type === 'button' && text(v.children).includes(label));

const at = new Date(2026, 9, 20, 14, 32).toISOString();
const assistant = (extra: Message = {}): Message => ({
	id: 7,
	role: 'assistant',
	content: 'Hello **world**',
	status: 'done',
	steps: [],
	created_at: at,
	...extra,
});
const user = (extra: Message = {}): Message => ({
	...assistant(),
	id: 3,
	role: 'user',
	...extra,
});

describe('AgentMessage', () => {
	beforeEach(() => {
		mockWriteClipboard.mockReset();
		jest.useFakeTimers();
	});
	afterEach(() => jest.useRealTimers());

	it('marks the root with the message id', () => {
		const root = mount(assistant()).tree();
		expect(root.props?.['data-message-id']).toBe(7);
	});

	it('shows local time with a full date tooltip on both roles', () => {
		for (const message of [assistant(), user()]) {
			const tree = mount(message).tree();
			const time = find(tree, (v) => v.type === 'time')!;
			expect(text(time.children)).toBe('14:32');
			expect(time.props?.title).toContain('Oct 20, 2026');
		}
	});

	it('hides the time while the timestamp is not set', () => {
		const tree = mount(assistant({ created_at: '' })).tree();
		expect(find(tree, (v) => v.type === 'time')).toBeUndefined();
		expect(text(tree)).not.toContain('Invalid');
	});

	it('shows the stats line under a finished reply and hides it when empty', () => {
		const withStats = mount(
			assistant({ duration_ms: 12400, input_tokens: 3210, output_tokens: 450 }),
		).tree();
		expect(text(withStats)).toContain('12.4 s · 3,210 in / 450 out tokens');
		const without = mount(
			assistant({ duration_ms: null, input_tokens: null, output_tokens: null }),
		).tree();
		expect(text(without)).not.toContain(' s ·');
		expect(text(without)).not.toContain('tokens');
	});

	it('renders quote chips that jump to available originals', () => {
		const quote = {
			message_id: 7,
			role: 'assistant',
			created_at: at,
			text: 'Hello',
			whole: false,
		};
		const { tree, emitted } = mount(user({ quotes: [quote] }), {
			isQuoteAvailable: () => true,
		});
		const chip = button(tree(), 'Hello')!;
		expect(text(chip.children)).toContain('AI');
		expect(text(chip.children)).toContain('14:32');
		(chip.props!.onClick as () => void)();
		expect(emitted).toEqual([['jump', quote]]);
	});

	it('shows unavailable originals as plain text without a jump', () => {
		const quote = {
			message_id: 99,
			role: 'user',
			created_at: at,
			text: 'Gone text',
			whole: true,
		};
		const tree = mount(user({ quotes: [quote] }), {
			isQuoteAvailable: () => false,
		}).tree();
		expect(text(tree)).toContain('Gone text');
		expect(text(tree)).toContain('Original message unavailable');
		expect(button(tree, 'Gone text')).toBeUndefined();
	});

	it('emits reply with the message', () => {
		const message = assistant();
		const { tree, emitted } = mount(message);
		(button(tree(), 'Reply')!.props!.onClick as () => void)();
		expect(emitted).toEqual([['reply', message]]);
	});

	it('copies the raw markdown and flips the label', async () => {
		mockWriteClipboard.mockResolvedValue(undefined);
		const { tree, bindings } = mount(user({ content: '**raw** md' }));
		await (button(tree(), 'Copy')!.props!.onClick as () => Promise<void>)();
		expect(mockWriteClipboard).toHaveBeenCalledWith('**raw** md');
		expect(bindings.copied).toBe(true);
	});

	it('does not flag copied when the clipboard write fails', async () => {
		mockWriteClipboard.mockRejectedValue(new Error('denied'));
		const { tree, bindings } = mount(assistant());
		await (button(tree(), 'Copy')!.props!.onClick as () => Promise<void>)();
		expect(bindings.copied).toBe(false);
	});

	it('offers no copy or reply while the reply is still running', () => {
		const tree = mount(assistant({ status: 'running', content: '' })).tree();
		expect(button(tree, 'Copy')).toBeUndefined();
		expect(button(tree, 'Reply')).toBeUndefined();
	});
});
