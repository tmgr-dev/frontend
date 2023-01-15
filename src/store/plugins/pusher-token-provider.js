import * as PusherPushNotifications from '@pusher/push-notifications-web';

export default function (token) {
	return new PusherPushNotifications.TokenProvider({
		url: process.env.VUE_APP_API_BASE_URL + 'pusher/beams-auth',
		headers: {
			Authorization: 'Bearer ' + (token ? token.token : null),
		},
	});
}
