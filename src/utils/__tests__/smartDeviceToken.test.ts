import {
	hasSmartDeviceToken,
	smartDeviceTokenState,
} from '@/utils/smartDeviceToken';

describe('hasSmartDeviceToken', () => {
	it('trusts the explicit flag from the backend', () => {
		expect(
			hasSmartDeviceToken({
				has_smart_device_token: true,
				smart_device_token: null,
			}),
		).toBe(true);
		expect(
			hasSmartDeviceToken({
				has_smart_device_token: false,
				smart_device_token: 'abc',
			}),
		).toBe(false);
	});

	it('falls back to the masked or legacy token field', () => {
		expect(hasSmartDeviceToken({ smart_device_token: '********' })).toBe(true);
		expect(
			hasSmartDeviceToken({ smart_device_token: 'raw-legacy-token' }),
		).toBe(true);
		expect(hasSmartDeviceToken({ smart_device_token: null })).toBe(false);
		expect(hasSmartDeviceToken({})).toBe(false);
		expect(hasSmartDeviceToken(null)).toBe(false);
	});
});

describe('smartDeviceTokenState', () => {
	it('is none when no token exists', () => {
		expect(smartDeviceTokenState({ has_smart_device_token: false }, null)).toBe(
			'none',
		);
	});

	it('is fresh right after generation', () => {
		expect(
			smartDeviceTokenState({ has_smart_device_token: true }, 'new-token'),
		).toBe('fresh');
	});

	it('is hidden when a token exists but was not just generated', () => {
		expect(smartDeviceTokenState({ has_smart_device_token: true }, null)).toBe(
			'hidden',
		);
		expect(
			smartDeviceTokenState({ smart_device_token: 'raw-legacy-token' }, null),
		).toBe('hidden');
	});
});
