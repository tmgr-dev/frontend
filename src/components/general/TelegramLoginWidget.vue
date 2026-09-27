<template>
	<div ref="widgetContainer" :id="widgetContainerId"></div>
</template>

<script setup lang="ts">
	import { nanoid } from 'nanoid'; // Using nanoid for unique IDs
	import { onMounted, onUnmounted, ref } from 'vue';

	const props = defineProps({
		botName: {
			type: String,
			required: true,
		},
		authUrl: {
			type: String,
			default: '',
		},
		callback: {
			type: Boolean,
			default: false,
		},
		widgetSize: {
			type: String,
			default: 'medium', // 'large', 'medium', 'small'
		},
	});

	const widgetContainerId = ref(`tg-widget-container-${nanoid(5)}`);
	const widgetScriptId = ref(`tg-widget-script-${nanoid(5)}`);
	const widgetContainer = ref<HTMLDivElement | null>(null);
	const emit = defineEmits<{ auth: [user: Record<string, unknown>] }>();
	const callbackName = `__tmgrTelegramAuth_${nanoid(8).replace(/[^A-Za-z0-9]/g, '')}`;

	const loadWidgetScript = () => {
		if (
			!document.getElementById(widgetScriptId.value) &&
			widgetContainer.value &&
			props.botName
		) {
			// Clean up any potential remnants from previous failed loads *within this container*
			widgetContainer.value.innerHTML = '';

			const script = document.createElement('script');
			script.id = widgetScriptId.value;
			script.async = true;
			script.src = 'https://telegram.org/js/telegram-widget.js?22';
			script.setAttribute('data-telegram-login', props.botName);
			script.setAttribute('data-size', props.widgetSize);
			if (props.callback) {
				(window as any)[callbackName] = (user: Record<string, unknown>) =>
					emit('auth', user);
				script.setAttribute('data-onauth', `${callbackName}(user)`);
			} else {
				script.setAttribute('data-auth-url', props.authUrl);
			}
			script.setAttribute('data-userpic', 'false');
			script.setAttribute('data-request-access', 'write');

			// Append to the component's container
			widgetContainer.value.appendChild(script);
			console.log(`Appended script ${script.id} to ${widgetContainerId.value}`);
		} else {
			console.log(
				`Script ${widgetScriptId.value} already exists or container not found.`,
			);
		}
	};

	const cleanupWidget = () => {
		delete (window as any)[callbackName];
		// Remove the script we added
		const scriptElement = document.getElementById(widgetScriptId.value);
		if (scriptElement) {
			scriptElement.remove();
			console.log(`Removed script ${widgetScriptId.value}`);
		}

		// Attempt to remove the iframe Telegram's script likely created
		// This assumes a naming pattern, which is fragile.
		const internalWidgetIframe = document.querySelector(
			`iframe[id^="telegram-login-${props.botName}"]`,
		);
		if (internalWidgetIframe) {
			internalWidgetIframe.remove();
			console.log(`Removed internal iframe ${internalWidgetIframe.id}`);
		}
		// Also clear our container just in case
		if (widgetContainer.value) {
			widgetContainer.value.innerHTML = '';
		}
	};

	onMounted(() => {
		console.log(`Mounting ${widgetContainerId.value}`);
		loadWidgetScript();
	});

	onUnmounted(() => {
		console.log(`Unmounting ${widgetContainerId.value}`);
		cleanupWidget();
	});

	// Optional: Watch for prop changes if needed, though might be less critical now
	// watch(() => [props.botName, props.authUrl, props.widgetSize], () => {
	//   cleanupWidget();
	//   loadWidgetScript();
	// });
</script>
