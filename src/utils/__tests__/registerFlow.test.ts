import { submitRegistration } from '@/utils/registerFlow';

const form = {
	name: 'Ada',
	email: 'ada@example.test',
	password: 'secret-pass',
	password_confirmation: 'secret-pass',
};
const httpError = (status: number, data: unknown = {}) => ({
	response: { status, data },
});

test('a mismatched confirmation never sends a request', async () => {
	const register = jest.fn();
	const result = await submitRegistration(
		{ ...form, password_confirmation: 'other' },
		register,
	);
	expect(register).not.toHaveBeenCalled();
	expect(result).toMatchObject({
		ok: false,
		errors: { password_confirmation: [expect.any(String)] },
	});
});

test('409 email_taken is reported as taken', async () => {
	const result = await submitRegistration(form, async () => {
		throw httpError(409, { error: 'email_taken' });
	});
	expect(result).toMatchObject({ ok: false, emailTaken: true, errors: {} });
	expect((result as any).message).toMatch(/already exists/);
});

test('429 is reported as too many attempts', async () => {
	const result = await submitRegistration(form, async () => {
		throw httpError(429);
	});
	expect(result).toMatchObject({ ok: false, emailTaken: false, errors: {} });
	expect((result as any).message).toMatch(/Too many attempts/);
});

test('no response is reported as offline', async () => {
	const result = await submitRegistration(form, async () => {
		throw new Error('Network Error');
	});
	expect((result as any).message).toMatch(/No connection/);
});

test('422 keeps field errors as an object', async () => {
	const result = await submitRegistration(form, async () => {
		throw httpError(422, { errors: { name: ['Required'] } });
	});
	expect(result).toMatchObject({ ok: false, errors: { name: ['Required'] } });
});

test('errors is always an object, even for an unknown status', async () => {
	const result = await submitRegistration(form, async () => {
		throw httpError(500, {});
	});
	expect((result as any).errors).toEqual({});
	expect((result as any).message).not.toBe('');
});

test('a retry after a failure succeeds and commits only on the successful call', async () => {
	const commit = jest.fn();
	const register = jest
		.fn()
		.mockRejectedValueOnce(new Error('Network Error'))
		.mockImplementationOnce(async () => commit('setToken'));
	const first = await submitRegistration(form, register);
	expect(first.ok).toBe(false);
	expect(commit).not.toHaveBeenCalled();
	const second = await submitRegistration(form, register);
	expect(second.ok).toBe(true);
	expect(commit).toHaveBeenCalledTimes(1);
});
