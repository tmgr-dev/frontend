import type { DeviceRequest } from '@/actions/tmgr/devices';
import {
	isCompleteUserCode,
	normalizeUserCode,
	toggleScope,
} from '@/utils/deviceLogin';
import { httpErrorInfo } from '@/utils/emailVerification';
import { computed, ref } from 'vue';

const retryAfterSeconds = (error: unknown): number => {
	const headers = (
		error as { response?: { headers?: Record<string, unknown> } }
	)?.response?.headers;
	const value = Number(headers?.['retry-after'] ?? headers?.['Retry-After']);
	return Number.isFinite(value) && value > 0 ? value : 0;
};

export type DeviceApprovalState =
	| 'entry'
	| 'loading'
	| 'ready'
	| 'submitting'
	| 'approved'
	| 'denied'
	| 'not_found'
	| 'locked'
	| 'error';

export const createDeviceApproval = (deps: {
	lookup: (code: string) => Promise<DeviceRequest>;
	approve: (code: string, scopes: string[]) => Promise<void>;
	deny: (code: string) => Promise<void>;
}) => {
	const state = ref<DeviceApprovalState>('entry');
	const code = ref('');
	const request = ref<DeviceRequest | null>(null);
	const selected = ref<string[]>([]);
	const retryAfter = ref(0);

	const setCode = (raw: string) => {
		code.value = normalizeUserCode(raw);
	};

	const canLookup = computed(
		() => isCompleteUserCode(code.value) && state.value !== 'loading',
	);
	const canApprove = computed(
		() => state.value === 'ready' && selected.value.length > 0,
	);

	const find = async () => {
		if (!isCompleteUserCode(code.value) || state.value === 'loading') return;
		state.value = 'loading';
		request.value = null;
		try {
			const found = await deps.lookup(code.value);
			request.value = found;
			selected.value = [...found.requested_scopes];
			state.value = 'ready';
		} catch (error) {
			fail(error);
		}
	};

	const fail = (error: unknown) => {
		const { status } = httpErrorInfo(error);
		if (status === 404) {
			state.value = 'not_found';
		} else if (status === 429) {
			retryAfter.value = retryAfterSeconds(error);
			state.value = 'locked';
		} else {
			state.value = 'error';
		}
	};

	const toggle = (scope: string) => {
		selected.value = toggleScope(selected.value, scope);
	};

	const approve = async () => {
		if (!canApprove.value) return;
		state.value = 'submitting';
		try {
			await deps.approve(code.value, selected.value);
			state.value = 'approved';
		} catch (error) {
			fail(error);
		}
	};

	const deny = async () => {
		if (state.value !== 'ready') return;
		state.value = 'submitting';
		try {
			await deps.deny(code.value);
			state.value = 'denied';
		} catch (error) {
			fail(error);
		}
	};

	const reset = () => {
		state.value = 'entry';
		request.value = null;
		selected.value = [];
		code.value = '';
	};

	const retry = () => {
		state.value = request.value ? 'ready' : 'entry';
	};

	return {
		state,
		code,
		request,
		selected,
		retryAfter,
		canLookup,
		canApprove,
		setCode,
		find,
		toggle,
		approve,
		deny,
		reset,
		retry,
	};
};
