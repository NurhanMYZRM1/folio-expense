"use dom";

import "../polyfills";
import "../folio.css";
import { useLocation } from "react-router-dom";
import { FinanceOverview, Reports } from "@folio/features/finance/Overview";
import { Accounts } from "@folio/features/finance/Accounts";
import { Transactions } from "@folio/features/finance/Transactions";
import { Budgets } from "@folio/features/finance/Budgets";
import { Business } from "@folio/features/finance/Business";
import { TaxTools } from "@folio/features/finance/TaxTools";
import { Bills } from "@folio/features/finance/Bills";
import { FolioHost, type FolioHostProps } from "../host";

function FinancePage() {
  const { pathname } = useLocation();
  switch (pathname) {
    case "/finance/business": return <Business />;
    case "/finance/tax": return <TaxTools />;
    case "/finance/accounts": return <Accounts />;
    case "/finance/transactions": return <Transactions />;
    case "/finance/budgets": return <Budgets />;
    case "/finance/bills": return <Bills />;
    case "/finance/reports": return <Reports />;
    default: return <FinanceOverview />;
  }
}
export default function FinanceScreen(props: FolioHostProps) {
  return <FolioHost {...props} pattern="/finance/*"><FinancePage /></FolioHost>;
}
