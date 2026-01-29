import Echo from 'laravel-echo';
import Pusher from 'pusher-js';

window.Pusher = Pusher;

export default function (token) {
	const scheme = import.meta.env.VITE_PUSHER_SCHEME || 'http';
	const port = parseInt(import.meta.env.VITE_PUSHER_PORT || '6001', 10);

	return new Echo({
		broadcaster: 'pusher',
		key: import.meta.env.VITE_PUSHER_KEY,
		cluster: 'mt1',
		wsHost: import.meta.env.VITE_PUSHER_HOST || 'localhost',
		wsPort: port,
		wssPort: port,
		forceTLS: scheme === 'https',
		disableStats: true,
		enabledTransports: ['ws', 'wss'],
		authEndpoint: import.meta.env.VITE_API_BASE_URL + 'broadcasting/auth',
		auth: {
			headers: {
				Accept: 'application/json',
				Authorization: 'Bearer ' + (token ? token.token : null),
			},
		},
	});
}
