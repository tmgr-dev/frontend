import Vue from 'vue'
import components from "./loadComponents";

components.map((component) => {
	return Vue.component(component.name, component)
})

export default Vue
