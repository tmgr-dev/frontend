import * as PusherPushNotifications from '@pusher/push-notifications-web';

let client = {};

if ('PushManager' in window) {
	client = new PusherPushNotifications.Client({
		instanceId: import.meta.env.VITE_PUSHER_BEAMS_INSTANCE_ID,
	});
}

export default client;
