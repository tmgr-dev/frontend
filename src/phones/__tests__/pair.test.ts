import { invoke } from '@tauri-apps/api/core';
import { phonesBeginPairing } from '../api';
import { pairIphone } from '../pair';

jest.mock('@tauri-apps/api/core', () => ({ invoke: jest.fn() }));
jest.mock('@tauri-apps/api/event', () => ({ listen: jest.fn() }));

const mailbox = { id: 'mb-1', desktop_token: 'dt', phone_token: 'pt' };

describe('pair an iPhone flow', () => {
	beforeEach(() => (invoke as jest.Mock).mockReset());

	it('creates the mailbox and begins pairing with its tokens', async () => {
		(invoke as jest.Mock).mockResolvedValue(undefined);
		const createMailbox = jest.fn().mockResolvedValue(mailbox);
		const result = await pairIphone(
			{
				createMailbox,
				begin: phonesBeginPairing,
				apiBase: 'https://api.example.com/api/',
				userId: 7,
			},
			'  Work Mac ',
		);
		expect(result).toEqual({ ok: true });
		expect(createMailbox).toHaveBeenCalledTimes(1);
		expect(invoke).toHaveBeenCalledWith('phones_begin_pairing', {
			apiBase: 'https://api.example.com/api/',
			userId: 7,
			mailboxId: 'mb-1',
			desktopToken: 'dt',
			phoneToken: 'pt',
			desktopName: 'Work Mac',
		});
	});

	it('shows the server message when the mailbox cannot be created', async () => {
		const begin = jest.fn();
		const result = await pairIphone(
			{
				createMailbox: jest
					.fn()
					.mockRejectedValue({ response: { data: { message: 'Too many' } } }),
				begin,
				apiBase: 'x',
				userId: 7,
			},
			'Mac',
		);
		expect(result).toEqual({ ok: false, error: 'Too many' });
		expect(begin).not.toHaveBeenCalled();
	});

	it('shows the native error when begin fails', async () => {
		const result = await pairIphone(
			{
				createMailbox: jest.fn().mockResolvedValue(mailbox),
				begin: jest.fn().mockRejectedValue('Untrusted API host'),
				apiBase: 'x',
				userId: 7,
			},
			'Mac',
		);
		expect(result).toEqual({ ok: false, error: 'Untrusted API host' });
	});

	it('refuses an empty name or a missing user without calling the API', async () => {
		const createMailbox = jest.fn();
		const base = { createMailbox, begin: jest.fn(), apiBase: 'x' };
		expect((await pairIphone({ ...base, userId: 7 }, '  ')).ok).toBe(false);
		expect((await pairIphone({ ...base, userId: undefined }, 'Mac')).ok).toBe(
			false,
		);
		expect(createMailbox).not.toHaveBeenCalled();
	});
});
