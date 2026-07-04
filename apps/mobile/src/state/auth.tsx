import {
	createContext,
	useContext,
	useEffect,
	useState,
	ReactNode,
	useCallback,
} from 'react';
import { getToken, setToken } from '../api/storage';
import { setUnauthorizedHandler } from '../api/client';
import * as authApi from '../api/auth';

type Status = 'loading' | 'authenticated' | 'unauthenticated';

type AuthContextValue = {
	status: Status;
	user: any | null;
	login: (email: string, password: string) => Promise<void>;
	register: (name: string, email: string, password: string) => Promise<void>;
	logout: () => Promise<void>;
	refreshUser: () => Promise<void>;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
	const [status, setStatus] = useState<Status>('loading');
	const [user, setUser] = useState<any | null>(null);

	const loadUser = useCallback(async () => {
		try {
			const me = await authApi.getMe();
			setUser(me);
			setStatus('authenticated');
		} catch {
			await setToken(null);
			setUser(null);
			setStatus('unauthenticated');
		}
	}, []);

	// Bootstrap from stored token; wire the 401 -> unauthenticated handler.
	useEffect(() => {
		setUnauthorizedHandler(() => {
			setUser(null);
			setStatus('unauthenticated');
		});
		(async () => {
			const token = await getToken();
			if (token) await loadUser();
			else setStatus('unauthenticated');
		})();
		return () => setUnauthorizedHandler(null);
	}, [loadUser]);

	const login = useCallback(
		async (email: string, password: string) => {
			await authApi.login(email, password);
			await loadUser();
		},
		[loadUser],
	);

	const register = useCallback(
		async (name: string, email: string, password: string) => {
			await authApi.register(name, email, password);
			await loadUser();
		},
		[loadUser],
	);

	const logout = useCallback(async () => {
		await authApi.logout();
		setUser(null);
		setStatus('unauthenticated');
	}, []);

	return (
		<AuthContext.Provider
			value={{ status, user, login, register, logout, refreshUser: loadUser }}
		>
			{children}
		</AuthContext.Provider>
	);
}

export function useAuth(): AuthContextValue {
	const ctx = useContext(AuthContext);
	if (!ctx) throw new Error('useAuth must be used within <AuthProvider>');
	return ctx;
}
