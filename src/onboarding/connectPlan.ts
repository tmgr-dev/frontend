import type { SmartDeviceTokenUser } from '@/utils/smartDeviceToken';
import { hasSmartDeviceToken } from '@/utils/smartDeviceToken';

export type ConnectPlan = 'mint' | 'choose' | 'reconnect' | 'user-error';

export interface ConnectPlanDeps {
	getUser(): Promise<unknown>;
	fetchFacts(keys: string[]): Promise<Record<string, unknown>>;
}

export async function planConnect(deps: ConnectPlanDeps): Promise<ConnectPlan> {
	let user: unknown;
	try {
		user = await deps.getUser();
	} catch {
		return 'user-error';
	}
	if (hasSmartDeviceToken(user as SmartDeviceTokenUser)) return 'choose';
	try {
		const facts = await deps.fetchFacts(['mcp_first_call_at']);
		if (facts?.mcp_first_call_at) return 'reconnect';
	} catch {
		/* unreadable facts: the user has no token to lose, minting is safe */
	}
	return 'mint';
}
