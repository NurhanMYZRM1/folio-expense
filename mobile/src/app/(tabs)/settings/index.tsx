import Screen from "~/dom/pages/settings";
import { useFolioDomProps } from "~/native/dom-props";

export default function Route() {
  return <Screen {...useFolioDomProps("/settings")} />;
}
