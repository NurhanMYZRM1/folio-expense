"use dom";

import "../polyfills";
import "../folio.css";
import { Dashboard } from "@folio/features/dashboard/Dashboard";
import { FolioHost, type FolioHostProps } from "../host";

export default function OverviewScreen(props: FolioHostProps) {
  return (
    <FolioHost {...props} pattern="/">
      <Dashboard />
    </FolioHost>
  );
}
