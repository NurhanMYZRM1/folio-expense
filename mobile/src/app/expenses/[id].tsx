import Screen from "~/dom/pages/expense-detail";
import { useFolioDomProps } from "~/native/dom-props";

export default function Route() {
  return <Screen {...useFolioDomProps("/expenses/:id")} />;
}
