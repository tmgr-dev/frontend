import { compileScript, parse } from '@vue/compiler-sfc';
import { readFileSync } from 'fs';
import { join } from 'path';
import ts from 'typescript';

import { updateUser } from '@/actions/tmgr/user';
import { toast } from '@/components/ui/toast';
import { t } from '@/i18n';

jest.mock('@/actions/tmgr/user', () => ({
	getUser: jest.fn(),
	updateUser: jest.fn(),
}));
jest.mock('@/actions/tmgr/avatars', () => ({}));
jest.mock('@/actions/tmgr/files', () => ({}));
jest.mock('@/actions/tmgr/personas', () => ({}));
jest.mock('@/components/ui/toast', () => ({ toast: jest.fn() }));
jest.mock('@/composable/useDocumentTitle', () => ({
	setDocumentTitle: jest.fn(),
}));
for (const path of [
	'@/components/devices/SignedInDevices.vue',
	'@/components/general/UserAvatar.vue',
	'@/components/layouts/PageContainer.vue',
	'@/components/layouts/PageHeader.vue',
	'@/components/layouts/SettingsSection.vue',
	'@/components/profile/AvatarCropModal.vue',
])
	jest.mock(path, () => ({ __esModule: true, default: {} }));
jest.mock('@/components/ui/button', () => ({ Button: {} }));
jest.mock('@/components/ui/input', () => ({ Input: {} }));
jest.mock('@/components/ui/label', () => ({ Label: {} }));

const filename = join(__dirname, '../Profile.vue');
const { descriptor } = parse(readFileSync(filename, 'utf8'), { filename });

const loadOptions = (): Record<string, any> => {
	const script = compileScript(descriptor, { id: 'profile' });
	const code = ts.transpileModule(script.content, {
		compilerOptions: {
			module: ts.ModuleKind.CommonJS,
			target: ts.ScriptTarget.ES2020,
		},
	}).outputText;
	const module = { exports: {} as Record<string, any> };
	new Function('require', 'module', 'exports', code)(
		require,
		module,
		module.exports,
	);
	return module.exports.default;
};

const options = loadOptions();

const instance = (user: Record<string, unknown> = {}) => {
	const vm: Record<string, any> = {
		...options.data(),
		$emit: jest.fn(),
		tokensRevokedNotice: '',
	};
	vm.user = { name: 'A', ...user };
	for (const [name, fn] of Object.entries(options.methods))
		vm[name] = (fn as Function).bind(vm);
	return vm;
};

const rejectWith = (error: unknown) =>
	(updateUser as jest.Mock).mockRejectedValueOnce(error);

beforeEach(() => jest.clearAllMocks());

describe('saveUser errors', () => {
	test('a response with only a message shows it in a destructive toast', async () => {
		rejectWith({
			response: { status: 403, data: { message: 'Forbidden here' } },
		});
		const vm = instance();
		await vm.saveUser();
		expect(toast).toHaveBeenCalledWith({
			title: t('settingsPages.profile.errorTitle'),
			description: 'Forbidden here',
			variant: 'destructive',
		});
		expect(vm.saving).toBe(false);
	});

	test('a network error shows the generic message', async () => {
		rejectWith(new Error('Network Error'));
		const vm = instance();
		await vm.saveUser();
		expect(toast).toHaveBeenCalledWith({
			title: t('settingsPages.profile.errorTitle'),
			description: t('settingsPages.profile.saveFailed'),
			variant: 'destructive',
		});
	});

	test('field errors render inline without a toast', async () => {
		rejectWith({
			response: {
				status: 422,
				data: { message: 'Invalid', errors: { name: ['Name is required'] } },
			},
		});
		const vm = instance();
		await vm.saveUser();
		expect(vm.errors).toEqual({ name: ['Name is required'] });
		expect(toast).not.toHaveBeenCalled();
	});

	test('a wrong current password still maps to the field without a toast', async () => {
		rejectWith({
			response: {
				status: 422,
				data: { message: 'Current password is incorrect' },
			},
		});
		const vm = instance({ password: 'secret-1' });
		await vm.saveUser();
		expect(vm.errors).toEqual({
			current_password: ['Current password is incorrect'],
		});
		expect(toast).not.toHaveBeenCalled();
	});
});

describe('sign out of other devices checkbox', () => {
	const findCheckbox = (node: any): any => {
		if (
			node.props?.some(
				(p: any) =>
					p.name === 'data-testid' &&
					p.value?.content === 'logout-other-sessions',
			)
		)
			return node;
		for (const child of node.children ?? []) {
			const found = findCheckbox(child);
			if (found) return found;
		}
		return undefined;
	};

	const findLabel = (node: any): any => {
		if (
			node.tag === 'label' &&
			node.props?.some(
				(p: any) =>
					p.name === 'for' &&
					p.value?.content === 'profile-logout-other-sessions',
			)
		)
			return node;
		for (const child of node.children ?? []) {
			const found = findLabel(child);
			if (found) return found;
		}
		return undefined;
	};

	test('keeps its test id', () => {
		expect(findCheckbox(descriptor.template!.ast)).toBeDefined();
	});

	test('is shown only while a new password is typed', () => {
		const label = findLabel(descriptor.template!.ast);
		const vIf = label.props.find((p: any) => p.type === 7 && p.name === 'if');
		expect(vIf?.exp?.content).toBe('user.password');
	});
});
