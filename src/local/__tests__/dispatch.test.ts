import { dispatchLocal } from '../dispatch';
import { LocalRouter } from '../router';
import type { LocalContext } from '../types';

const ctx = {} as LocalContext;

const router = new LocalRouter().add('GET', 'tasks/:id/comments', () => ({
	ok: true,
}));

describe('dispatchLocal malformed path', () => {
	it('answers 400 when a path segment has a malformed percent-escape', async () => {
		await expect(
			dispatchLocal(router, ctx, 'GET', 'tasks/%E0%A4%A/comments'),
		).resolves.toEqual({ status: 400, data: { message: 'Malformed path' } });
	});

	it('still routes a well-formed path', async () => {
		await expect(
			dispatchLocal(router, ctx, 'GET', 'tasks/7/comments'),
		).resolves.toEqual({ status: 200, data: { data: { ok: true } } });
	});
});
