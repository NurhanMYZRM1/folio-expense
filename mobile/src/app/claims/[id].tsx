import Screen from "~/dom/pages/claim-detail";
import { useFolioDomProps } from "~/native/dom-props";

export default function Route() {
  return <Screen {...useFolioDomProps("/claims/:id")} />;
}
