import { useErrorHandler } from '../useErrorHandler';

test('normalizes recognized and unexpected error classifications', () => {
	const log = jest.spyOn(console, 'error').mockImplementation(() => {});
	try {
		const errors = useErrorHandler();
		expect(
			errors.handleError({ message: 'Expired session', type: 'authentication' })
				.type,
		).toBe('authentication');
		expect(errors.errorMessage.value).toContain('log in again');
		expect(
			errors.handleError({
				message: 'Unexpected',
				type: 'unrecognized-service-code',
			}).type,
		).toBe('unknown');
		expect(errors.isServerError({ message: 'No status', timestamp: '' })).toBe(
			false,
		);
		expect(
			errors.isClientError({
				message: 'Bad request',
				timestamp: '',
				status: 400,
			}),
		).toBe(true);
	} finally {
		log.mockRestore();
	}
});
