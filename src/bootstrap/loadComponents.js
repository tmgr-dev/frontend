import BaseLayout from "../components/Layouts/BaseLayout";
import Modal from "../components/Layouts/Modal";
import Navbar from "../components/UIElements/Navbar";
import Button from "../components/UIElements/Button";
import DropdownMenu from "../components/UIElements/DropdownMenu";
import Countdown from "../components/Tasks/Countdown";
import InputField from "../components/UIElements/InputField";
import MiniLoader from "../components/UIElements/MiniLoader";
import DayNightSwitch from "../components/UIElements/DayNightSwitch";
import RockerSwitch from 'vue-rocker-switch';
import "vue-rocker-switch/dist/vue-rocker-switch.css";

export default [
    BaseLayout,
    RockerSwitch,
    DayNightSwitch,
    Modal,
    Navbar,
    DropdownMenu,
    Countdown,
    InputField,
    Button,
    MiniLoader
]
