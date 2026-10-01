import {
	createLinkConfirm,
	createResendController,
	createVerifyEmail,
} from '../useEmailVerification';

const httpError = (status: number, data: Record<string, unknown>) => ({
	isAxiosError: true,
	response: { status, data },
});

describe('verify email state machine', () => {
	const setup = (verify: jest.Mock, resend: jest.Mock = jest.fn()) => {
		const refreshUser = jest.fn().mockResolvedValue(undefined);
		const machine = createVerifyEmail({ verify, resend, refreshUser });
		return { machine, refreshUser, resend };
	};

	it('is invalid without a token and never calls the API', async () => {
		const verify = jest.fn();
		const { machine } = setup(verify);
		await machine.run('');
		expect(machine.state.value).toBe('invalid');
		expect(verify).not.toHaveBeenCalled();
	});

	it('goes verifying then success and refreshes the user', async () => {
		let release: (v: unknown) => void = () => {};
		const verify = jest.fn(() => new Promise((resolve) => (release = resolve)));
		const { machine, refreshUser } = setup(verify);
		const pending = machine.run('abc');
		expect(machine.state.value).toBe('verifying');
		release({ status: 'verified' });
		await pending;
		expect(verify).toHaveBeenCalledWith('abc');
		expect(machine.state.value).toBe('success');
		expect(refreshUser).toHaveBeenCalledTimes(1);
	});

	it('reports already verified and refreshes the user', async () => {
		const { machine, refreshUser } = setup(
			jest.fn().mockResolvedValue({ status: 'already_verified' }),
		);
		await machine.run('abc');
		expect(machine.state.value).toBe('already_verified');
		expect(refreshUser).toHaveBeenCalled();
	});

	it('still succeeds when the user refresh fails', async () => {
		const verify = jest.fn().mockResolvedValue({ status: 'verified' });
		const refreshUser = jest.fn().mockRejectedValue(new Error('x'));
		const machine = createVerifyEmail({
			verify,
			resend: jest.fn(),
			refreshUser,
		});
		await machine.run('abc');
		expect(machine.state.value).toBe('success');
	});

	it.each([
		[422, 'expired_token', 'expired'],
		[422, 'invalid_token', 'invalid'],
		[403, 'wrong_account', 'wrong_account'],
	])('maps %s %s to %s', async (status, error, expected) => {
		const { machine, refreshUser } = setup(
			jest.fn().mockRejectedValue(httpError(status, { error })),
		);
		await machine.run('abc');
		expect(machine.state.value).toBe(expected);
		expect(refreshUser).not.toHaveBeenCalled();
	});

	it('falls back to a generic error state for unknown failures', async () => {
		const { machine } = setup(jest.fn().mockRejectedValue(new Error('net')));
		await machine.run('abc');
		expect(machine.state.value).toBe('error');
	});

	it('resends from the expired state and reports 429 retry time', async () => {
		const resend = jest
			.fn()
			.mockRejectedValueOnce(
				httpError(429, { error: 'too_many_requests', retry_after: 42 }),
			)
			.mockResolvedValueOnce({ status: 'sent' });
		const { machine } = setup(
			jest.fn().mockRejectedValue(httpError(422, { error: 'expired_token' })),
			resend,
		);
		await machine.run('abc');
		await machine.resend.send();
		expect(machine.resend.state.value).toBe('rate_limited');
		expect(machine.resend.retryAfter.value).toBe(42);
		await machine.resend.send();
		expect(machine.resend.state.value).toBe('sent');
	});
});

describe('resend controller', () => {
	it('handles sent, already_verified, rate limit and failure', async () => {
		const resend = jest
			.fn()
			.mockResolvedValueOnce({ status: 'sent' })
			.mockResolvedValueOnce({ status: 'already_verified' })
			.mockRejectedValueOnce(
				httpError(429, { error: 'too_many_requests', retry_after: 30 }),
			)
			.mockRejectedValueOnce(new Error('boom'));
		const onAlreadyVerified = jest.fn();
		const c = createResendController({ resend, onAlreadyVerified });
		await c.send();
		expect(c.state.value).toBe('sent');
		await c.send();
		expect(c.state.value).toBe('already_verified');
		expect(onAlreadyVerified).toHaveBeenCalledTimes(1);
		await c.send();
		expect(c.state.value).toBe('rate_limited');
		expect(c.retryAfter.value).toBe(30);
		await c.send();
		expect(c.state.value).toBe('error');
	});

	it('ignores a second click while sending', async () => {
		let release: (v: { status: string }) => void = () => {};
		const resend = jest.fn(
			() => new Promise<{ status: string }>((r) => (release = r)),
		);
		const c = createResendController({ resend });
		const first = c.send();
		await c.send();
		expect(resend).toHaveBeenCalledTimes(1);
		release({ status: 'sent' });
		await first;
	});
});

describe('social link confirm', () => {
	it('links on success and exposes the provider', async () => {
		const confirm = jest
			.fn()
			.mockResolvedValue({ status: 'linked', provider: 'google' });
		const m = createLinkConfirm({ confirm });
		const pending = m.run('t');
		expect(m.state.value).toBe('confirming');
		await pending;
		expect(confirm).toHaveBeenCalledWith('t');
		expect(m.state.value).toBe('linked');
		expect(m.provider.value).toBe('google');
	});

	it.each([
		['expired_token', 'expired'],
		['invalid_token', 'invalid'],
	])('maps %s to %s', async (error, expected) => {
		const m = createLinkConfirm({
			confirm: jest.fn().mockRejectedValue(httpError(422, { error })),
		});
		await m.run('t');
		expect(m.state.value).toBe(expected);
	});

	it('is invalid without a token', async () => {
		const confirm = jest.fn();
		const m = createLinkConfirm({ confirm });
		await m.run('');
		expect(m.state.value).toBe('invalid');
		expect(confirm).not.toHaveBeenCalled();
	});

	it('shows a generic error for unknown failures', async () => {
		const m = createLinkConfirm({
			confirm: jest.fn().mockRejectedValue(new Error('x')),
		});
		await m.run('t');
		expect(m.state.value).toBe('error');
	});
});
