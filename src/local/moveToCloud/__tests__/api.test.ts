import {
	apiErrorMessage,
	createImportApi,
	isUnsupportedSchema,
	UNSUPPORTED_SCHEMA_MESSAGE,
} from '../api';

const http = () => {
	const calls: [string, string, unknown?][] = [];
	return {
		calls,
		client: {
			get: jest.fn(
				async (url: string) => (
					calls.push(['GET', url]), { data: { ok: 'get' } }
				),
			),
			post: jest.fn(
				async (url: string, body?: unknown) => (
					calls.push(['POST', url, body]), { data: { ok: 'post' } }
				),
			),
			delete: jest.fn(
				async (url: string) => (calls.push(['DELETE', url]), { data: {} }),
			),
		},
	};
};

describe('import api', () => {
	it('hits the contract routes', async () => {
		const { client, calls } = http();
		const api = createImportApi(client as any);
		await api.capabilities();
		await api.createJob({ fingerprint: 'f' } as any);
		await api.bundleUploaded('j');
		await api.presignFile('j', { sha256: 's', size: 1, mime: 'a/b' });
		await api.confirmFile('j', 's');
		await api.commit('j');
		await api.status('j');
		await api.retry('j');
		await api.publish('j');
		await api.publish('j', true);
		await api.discard('j');
		expect(calls).toEqual([
			['GET', '/imports/local-workspace/capabilities'],
			['POST', '/imports/local-workspace', { fingerprint: 'f' }],
			['POST', '/imports/j/bundle/uploaded', undefined],
			[
				'POST',
				'/imports/j/files/presign',
				{ sha256: 's', size: 1, mime: 'a/b' },
			],
			['POST', '/imports/j/files/confirm', { sha256: 's' }],
			['POST', '/imports/j/commit', undefined],
			['GET', '/imports/j'],
			['POST', '/imports/j/retry', undefined],
			['POST', '/imports/j/publish', {}],
			['POST', '/imports/j/publish', { accept_differences: true }],
			['DELETE', '/imports/j'],
		]);
	});

	it('recognises the unsupported schema refusal', () => {
		const error = {
			response: {
				status: 422,
				data: { error: 'unsupported_bundle_schema', message: 'x' },
			},
		};
		expect(isUnsupportedSchema(error)).toBe(true);
		expect(apiErrorMessage(error)).toBe(UNSUPPORTED_SCHEMA_MESSAGE);
		expect(
			isUnsupportedSchema({
				response: { status: 422, data: { error: 'other' } },
			}),
		).toBe(false);
	});

	it('prefers the server message for other errors', () => {
		expect(
			apiErrorMessage({
				response: { status: 429, data: { message: 'Too many moves today.' } },
			}),
		).toBe('Too many moves today.');
		expect(apiErrorMessage(new Error('boom'))).toBe('boom');
	});
});
