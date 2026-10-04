"use dom";

import "../polyfills";
import "../folio.css";
import { Settings } from "@folio/features/settings/Settings";
import { FolioHost, type FolioHostProps } from "../host";

export default function SettingsScreen(props: FolioHostProps) {
  return (
    <FolioHost {...props} pattern="/settings">
      <Settings />
    </FolioHost>
  );
}
