import { waitForUser } from '../account';

describe('waitForUser', () => {
	it('answers at once when the user is already loaded', async () => {
		const sleep = jest.fn(async () => {});
		await expect(
			waitForUser(() => ({ userId: 7, signedIn: true }), sleep),
		).resolves.toEqual({ known: true, userId: 7 });
		expect(sleep).not.toHaveBeenCalled();
	});

	it('confirms nobody is signed in without waiting', async () => {
		const sleep = jest.fn(async () => {});
		await expect(
			waitForUser(() => ({ userId: null, signedIn: false }), sleep),
		).resolves.toEqual({ known: true, userId: null });
		expect(sleep).not.toHaveBeenCalled();
	});

	it('waits for a session that is still loading', async () => {
		let polls = 0;
		const sleep = jest.fn(async () => void polls++);
		const result = await waitForUser(
			() => ({ userId: polls >= 3 ? 7 : null, signedIn: true }),
			sleep,
		);
		expect(result).toEqual({ known: true, userId: 7 });
		expect(sleep).toHaveBeenCalledTimes(3);
	});

	it('gives up as unknown, not as another account, when the user never loads', async () => {
		const result = await waitForUser(
			() => ({ userId: null, signedIn: true }),
			async () => {},
			1000,
		);
		expect(result).toEqual({ known: false, userId: null });
	});
});
