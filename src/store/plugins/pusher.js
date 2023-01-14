import Echo from 'laravel-echo';

window.Pusher = require('pusher-js');

export default function(token) {
	return new Echo({
		broadcaster: 'pusher',
		key: process.env.VUE_APP_PUSHER_KEY,
		cluster: process.env.VUE_APP_PUSHER_CLUSTER,
		forceTLS: true,
		authEndpoint: process.env.VUE_APP_API_BASE_URL + 'broadcasting/auth',
		auth: {
			headers: {
				Accept: 'application/json',
				Authorization: 'Bearer ' + (token ? token.token : null)
			}
		}
	});
}
