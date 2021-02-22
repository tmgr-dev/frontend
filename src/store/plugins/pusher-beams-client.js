import * as PusherPushNotifications from "@pusher/push-notifications-web";
export default new PusherPushNotifications.Client({
	instanceId: process.env.VUE_APP_PUSHER_BEAMS_INSTANCE_ID,
})
