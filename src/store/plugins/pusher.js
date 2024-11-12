import Echo from 'laravel-echo';
import Pusher from 'pusher-js';

window.Pusher = Pusher;

export default function (token) {
	return new Echo({
		broadcaster: 'pusher',
		key: import.meta.env.VITE_PUSHER_KEY,
		cluster: import.meta.env.VITE_PUSHER_CLUSTER,
		forceTLS: true,
		authEndpoint: import.meta.env.VITE_API_BASE_URL + 'broadcasting/auth',
		auth: {
			headers: {
				Accept: 'application/json',
				Authorization: 'Bearer ' + (token ? token.token : null),
			},
		},
	});
}
