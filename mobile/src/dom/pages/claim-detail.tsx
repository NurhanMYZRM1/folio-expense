"use dom";

import "../polyfills";
import "../folio.css";
import { ClaimDetail } from "@folio/features/claims/Claims";
import { FolioHost, type FolioHostProps } from "../host";

export default function ClaimDetailScreen(props: FolioHostProps) {
  return (
    <FolioHost {...props} pattern="/claims/:id">
      <ClaimDetail />
    </FolioHost>
  );
}
