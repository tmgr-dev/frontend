import AsyncStorage from '@react-native-async-storage/async-storage';

const TOKEN_KEY = 'tmgr.auth.token';

// The backend returns a raw JWT string; we persist just that.
export async function getToken(): Promise<string | null> {
	try {
		return await AsyncStorage.getItem(TOKEN_KEY);
	} catch {
		return null;
	}
}

export async function setToken(token: string | null): Promise<void> {
	try {
		if (token == null) await AsyncStorage.removeItem(TOKEN_KEY);
		else await AsyncStorage.setItem(TOKEN_KEY, token);
	} catch {
		// ignore storage errors
	}
}
