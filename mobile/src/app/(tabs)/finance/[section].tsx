import Screen from "~/dom/pages/finance";
import { useFolioDomProps } from "~/native/dom-props";
export default function Route() { return <Screen {...useFolioDomProps("/finance/:section")} />; }
