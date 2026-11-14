/** @jest-environment jsdom */
import { UNDO_WINDOW_MS } from '@/utils/memory';
import {
	buttonWithText,
	byTestId,
	flush,
	loadSfc,
	mountComponent,
} from './harness';

const mockAccept = jest.fn();
const mockReject = jest.fn();
const mockUndo = jest.fn();
const mockUpdate = jest.fn();
const mockToast = jest.fn();

jest.mock('@/plugins/axios', () => ({ __esModule: true, default: {} }));
jest.mock('@/actions/tmgr/memory', () => ({
	...jest.requireActual('@/actions/tmgr/memory'),
	acceptMemoryProposal: (...args: unknown[]) => mockAccept(...args),
	rejectMemoryProposal: (...args: unknown[]) => mockReject(...args),
	undoMemoryProposal: (...args: unknown[]) => mockUndo(...args),
	updateMemoryProposal: (...args: unknown[]) => mockUpdate(...args),
}));
jest.mock('@/components/ui/toast', () => ({
	ToastAction: 'ToastAction',
	useToast: () => ({ toast: (...args: unknown[]) => mockToast(...args) }),
}));
jest.mock('@/components/ui/button', () => {
	const { h } = jest.requireActual('vue');
	return {
		Button: {
			render(this: any) {
				return h('button', null, this.$slots.default?.());
			},
		},
	};
});
jest.mock('@/components/ui/textarea', () => {
	const { h } = jest.requireActual('vue');
	return {
		Textarea: {
			props: ['modelValue'],
			emits: ['update:modelValue'],
			render(this: any) {
				return h('textarea', {
					value: this.modelValue,
					onInput: (e: any) => this.$emit('update:modelValue', e.target.value),
				});
			},
		},
	};
});

const proposal = (extra: Record<string, unknown> = {}) => ({
	id: 21,
	topic: 'decisions',
	fact: 'Deploys go through the release branch',
	source: 'TM-12',
	check_by: null,
	origin: 'summarize',
	status: 'pending',
	auto_add_at: null,
	task_id: 12,
	created_at: '2026-11-14T10:00:00Z',
	entry_id: null,
	kind: 'add',
	target_entry_id: null,
	target_text: null,
	run_id: 5,
	...extra,
});

const Card = loadSfc('SummarizeDraftCard.vue');

const mountCard = async (p: Record<string, unknown>) => {
	const changed = jest.fn();
	const mounted = mountComponent(Card, {
		proposal: p,
		workspaceId: 3,
		workspaceCode: 'ws',
		onChanged: changed,
	});
	await flush();
	return { ...mounted, changed };
};

beforeEach(() => {
	mockAccept.mockReset();
	mockReject.mockReset();
	mockUndo.mockReset();
	mockUpdate.mockReset();
	mockToast.mockReset();
	mockAccept.mockResolvedValue({});
	mockReject.mockResolvedValue({ outcome: 'rejected', proposal: {} });
	mockUndo.mockResolvedValue({});
	document.body.innerHTML = '';
});

test('addCard_hasAddEditSkip', async () => {
	const { host, unmount } = await mountCard(proposal());
	expect(buttonWithText(host, 'Add')).toBeDefined();
	expect(buttonWithText(host, 'Edit')).toBeDefined();
	expect(buttonWithText(host, 'Skip')).toBeDefined();
	expect(host.textContent).toContain('Deploys go through the release branch');
	unmount();
});

test('replaceCard_showsDiff_applyCallsAccept', async () => {
	const { host, unmount, changed } = await mountCard(
		proposal({
			kind: 'replace',
			target_entry_id: 'e1',
			target_text: 'Deploys go through main',
		}),
	);
	expect(byTestId(host, 'draft-old')!.textContent).toContain(
		'Deploys go through main',
	);
	expect(byTestId(host, 'draft-new')!.textContent).toContain(
		'Deploys go through the release branch',
	);
	buttonWithText(host, 'Apply')!.click();
	await flush();
	expect(mockAccept).toHaveBeenCalledWith(3, 21);
	expect(changed).toHaveBeenCalled();
	unmount();
});

test('deleteCard_undoWithin10s', async () => {
	const { host, unmount } = await mountCard(
		proposal({
			kind: 'delete',
			fact: 'duplicate of another entry',
			target_entry_id: 'e1',
			target_text: 'Old fact',
		}),
	);
	const old = byTestId(host, 'draft-old')!;
	expect(old.textContent).toContain('Old fact');
	expect(old.className).toContain('line-through');
	expect(host.textContent).toContain('duplicate of another entry');
	expect(buttonWithText(host, 'Apply')).toBeUndefined();

	buttonWithText(host, 'Delete')!.click();
	await flush();
	expect(mockAccept).toHaveBeenCalledWith(3, 21);
	const toast = mockToast.mock.calls[0][0];
	expect(toast.duration).toBe(UNDO_WINDOW_MS);
	expect(UNDO_WINDOW_MS).toBe(10000);
	await toast.action.props.onClick();
	expect(mockUndo).toHaveBeenCalledWith(3, 21);
	unmount();
});

test('skip rejects and reports the change', async () => {
	const { host, unmount, changed } = await mountCard(proposal());
	buttonWithText(host, 'Skip')!.click();
	await flush();
	expect(mockReject).toHaveBeenCalledWith(3, 21);
	expect(changed).toHaveBeenCalled();
	unmount();
});

test('edit saves the new text through the update call', async () => {
	const { host, unmount } = await mountCard(proposal());
	buttonWithText(host, 'Edit')!.click();
	await flush();
	const area = host.querySelector('textarea') as HTMLTextAreaElement;
	area.value = 'Better text';
	area.dispatchEvent(new Event('input'));
	await flush();
	buttonWithText(host, 'Save')!.click();
	await flush();
	expect(mockUpdate).toHaveBeenCalledWith(3, 21, { fact: 'Better text' });
	unmount();
});

test('entryChanged_showsRefresh', async () => {
	mockAccept.mockRejectedValue({
		response: { status: 409, data: { error: 'entry_changed' } },
	});
	const { host, unmount, changed } = await mountCard(
		proposal({
			kind: 'replace',
			target_entry_id: 'e1',
			target_text: 'Deploys go through main',
		}),
	);
	buttonWithText(host, 'Apply')!.click();
	await flush();
	expect(host.textContent).toContain('Entry changed, refresh');
	expect(buttonWithText(host, 'Apply')).toBeUndefined();
	expect(changed).not.toHaveBeenCalled();
	buttonWithText(host, 'Refresh')!.click();
	await flush();
	expect(changed).toHaveBeenCalledTimes(1);
	unmount();
});
