export interface SmartDeviceTokenUser {
	has_smart_device_token?: boolean | null;
	smart_device_token?: string | null;
}

export type SmartDeviceTokenState = 'none' | 'fresh' | 'hidden';

export function hasSmartDeviceToken(
	user: SmartDeviceTokenUser | null | undefined,
): boolean {
	return user?.has_smart_device_token ?? !!user?.smart_device_token;
}

export function smartDeviceTokenState(
	user: SmartDeviceTokenUser | null | undefined,
	freshToken: string | null,
): SmartDeviceTokenState {
	if (freshToken) return 'fresh';
	return hasSmartDeviceToken(user) ? 'hidden' : 'none';
}
