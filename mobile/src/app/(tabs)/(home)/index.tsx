import Screen from "~/dom/pages/overview";
import { useFolioDomProps } from "~/native/dom-props";

export default function Route() {
  return <Screen {...useFolioDomProps("/")} />;
}
