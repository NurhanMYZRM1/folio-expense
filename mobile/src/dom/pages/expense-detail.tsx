"use dom";

import "../polyfills";
import "../folio.css";
import { ExpenseDetail } from "@folio/features/expenses/ExpenseDetail";
import { FolioHost, type FolioHostProps } from "../host";

export default function ExpenseDetailScreen(props: FolioHostProps) {
  return (
    <FolioHost {...props} pattern="/expenses/:id">
      <ExpenseDetail />
    </FolioHost>
  );
}
