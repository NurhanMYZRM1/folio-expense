"use dom";

import "../polyfills";
import "../folio.css";
import { Claims } from "@folio/features/claims/Claims";
import { FolioHost, type FolioHostProps } from "../host";

export default function ClaimsScreen(props: FolioHostProps) {
  return (
    <FolioHost {...props} pattern="/claims">
      <Claims />
    </FolioHost>
  );
}
