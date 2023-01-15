import selectable, { objectAssignSimple } from './selectable';

const objectAssign = Object.assign || objectAssignSimple;

function initSelectable(el, params, arg) {
	el.selectable = new selectable(
		objectAssign(
			{
				boundingBox: !!params.constraint
					? document.querySelector(params.constraint)
					: el,
				selectBoxSelector: params.box || '.selection',
				boundingBoxSelector: params.constraint,
				el,
			},
			arg,
		),
	);
	el.selectable.setSelectables(
		Array.prototype.slice.call(
			el.querySelectorAll(params.items || '.selectable'),
		),
	);
}

export default {
	name: 'selectable',
	config: {},
	install(app, installOptions) {
		app.directive('selectable', {
			twoWay: false,

			params: ['items', 'box', 'constraint'],

			beforeMount(el, binding) {
				let arg, params;
				if (!!el && !!binding) {
					// Vue.js v2
					arg = binding.value;
					params = el.dataset;
					initSelectable(el, params, arg);
				}
			},

			updated(el, binding, vnode, oldVnode) {
				if (!!el && !el.selectable) {
					// Vue.js v1 - init selectable
					initSelectable(el, el.dataset, binding.value);
				}
			},

			beforeUnmount(el) {
				if (!el) {
					el = this.el;
				}
				el.selectable.detach();
				el.selectable = null;
			},
		});
	},
};

/**
 *
 * @param {*} vnode component's properties
 * @param {*} oldvnode component's previous properties
 * @return boolean
 */
function hasUpdated(value, oldValue) {
	let updated = false;

	if (typeof value === 'string' && value !== oldValue) {
		updated = true;
	} else if (isObject(value)) {
		Object.keys(value).forEach((prop) => {
			if (value[prop] !== oldValue[prop]) {
				updated = true;
			}
		});
	}
	return updated;
}

/**
 * Allows to change internal selectable items list
 * @param {HTMLElement} el Element where v-selectable directive applied
 * @param {string} itemSelector (optional) CSS selector of elements to be used as selectable items
 * @return {number} number of selectable items or -1 if no selectable component found
 */
export function setSelectableItems(el, itemSelector) {
	if (
		!!el &&
		!!el.selectable &&
		typeof el.selectable.setSelectables === 'function'
	) {
		let items = Array.prototype.slice.call(
			el.querySelectorAll(itemSelector || el.dataset.items || '.selectable'),
		);
		el.selectable.setSelectables(items);
		return items.length;
	} else {
		return -1;
	}
}

/**
 * Sets options to directive
 * @param {HTMLElement} el Element where v-selectable directive applied
 * @param {object} options
 */
export function setOptions(el, options) {
	if (
		!!el &&
		!!el.selectable &&
		typeof el.selectable.setSelectables === 'function'
	) {
		const needsAttach =
			el.selectable.rootElement == null && options.rootElement != null;
		objectAssign(el.selectable, options);
		if (needsAttach) {
			el.selectable.attach();
		}
	}
}
