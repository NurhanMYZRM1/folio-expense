import Screen from "~/dom/pages/claims";
import { useFolioDomProps } from "~/native/dom-props";

export default function Route() {
  return <Screen {...useFolioDomProps("/claims")} />;
}
