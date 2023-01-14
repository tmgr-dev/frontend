import masker from './masker';
import tokens from './tokens';

// https://developer.mozilla.org/en-US/docs/Web/Guide/Events/Creating_and_triggering_events#The_old-fashioned_way
function event(name) {
	const evt = document.createEvent('Event');
	evt.initEvent(name, true, true);
	return evt;
}

export default function(el, binding) {
	let config = binding.value;
	if (Array.isArray(config) || typeof config === 'string') {
		config = {
			mask: config,
			tokens: tokens
		};
	}

	if (el.tagName.toLocaleUpperCase() !== 'INPUT') {
		const els = el.getElementsByTagName('input');
		if (els.length !== 1) {
			throw new Error('v-mask directive requires 1 input, found ' + els.length);
		} else {
			el = els[0];
		}
	}

	el.oninput = function(evt) {
		if (!evt.isTrusted) {
			return;
		}
		// avoid infinite loop
		/* other properties to try to diferentiate InputEvent of Event (custom)
		InputEvent (native)
			cancelable: false
			isTrusted: true

			composed: true
			isComposing: false
			which: 0

		Event (custom)
			cancelable: true
			isTrusted: false
		*/
		// by default, keep cursor at same position as before the mask
		let position = el.selectionEnd;
		// save the character just inserted
		const digit = el.value[position - 1];
		el.value = masker(el.value, config.mask, true, config.tokens);
		// if the digit was changed, increment position until find the digit again
		while (position < el.value.length && el.value.charAt(position - 1) !== digit) {
			position++;
		}
		if (el === document.activeElement) {
			el.setSelectionRange(position, position);
			setTimeout(function() {
				el.setSelectionRange(position, position);
			}, 0);
		}
		el.dispatchEvent(event('input'));
	};

	const newDisplay = masker(el.value, config.mask, true, config.tokens);

	if (newDisplay === el.value) {
		return;
	}

	el.value = newDisplay;
	el.dispatchEvent(event('input'));
}
