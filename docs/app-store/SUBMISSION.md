# App Store submission materials (iPhone and iPad), DRAFT

Drafted 5 November 2026 for Folio 0.1.0 (`mobile/`, bundle ID `com.folio.expenses`). Everything here is written from the app's verified behaviour; check it again against the build you upload. `[[…]]` marks facts only you can supply. **Each placeholder is a submission blocker.** Paste the text into App Store Connect yourself; nothing here has been submitted.

## App information

| Field              | Draft                                                                                   | Limit / note                                                                                          |
| ------------------ | --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Name               | Folio – Expense Claims                                                                  | 30 characters (2.3.7). "Folio" alone may already be taken: check availability, and trademarks (5.2.1) |
| Subtitle           | Receipts to claims, on device                                                           | 30 characters                                                                                         |
| Primary category   | Business                                                                                | Owner decision; Finance is the alternative. It is not a banking or payment service (5.1.1(ix))        |
| Secondary category | Finance                                                                                 | Optional                                                                                              |
| Price              | Free [[confirm]]                                                                        | No in-app purchases exist                                                                             |
| Availability       | [[territories, Malaysia first]]                                                         | EU storefronts need a Digital Services Act trader declaration                                         |
| Privacy Policy URL | [[https://… hosting docs/legal/PRIVACY.md]]                                             | Required (5.1.1(i)). Also set `extra.privacyPolicyUrl` in `mobile/app.json`                           |
| Support URL        | [[https://… with a contact method]]                                                     | Required (1.5). Also set `extra.supportUrl`                                                           |
| Copyright          | [[2026 legal name]]                                                                     |                                                                                                       |
| Age rating         | 4+ (answers below)                                                                      | 2.3.6                                                                                                 |
| Licence agreement  | Apple's standard EULA, or the reviewed `docs/legal/TERMS.md` with Apple's minimum terms | Owner / legal decision                                                                                |

### Promotional text (170)

Photograph a receipt and Folio reads it on your iPhone, ready for your expense claim. No account, no cloud: your receipts stay on your device.

### Description

Folio turns receipts into expense claims, privately on your iPhone or iPad.

SCAN OR IMPORT
• Take a photo, choose from Photos, or pick PDFs and images from Files (including iCloud Drive).
• Folio reads the merchant, premises address, date, total, tax and currency with Apple's on-device text recognition, and suggests a category.
• Clean receipts whose figures add up are marked ready; anything unclear waits for your review next to the receipt image.

REVIEW AND CORRECT
• Edit any field. Your manual corrections are kept even if a receipt is read again.
• Foreign receipts can be converted to your default currency at the European Central Bank's published rate for the receipt's date.

BUILD CLAIMS
• Group expenses into a claim and export a PDF report (with an optional receipt appendix), a CSV, or an Excel file, then share it with Mail, Files or any app.

PRIVATE BY DESIGN
• No account, no sign-in, no ads, no tracking.
• Receipts, expenses and claims are stored only on your device.
• Optional: connect your own OpenAI-compatible AI provider with your own API key to read difficult receipts. It is off until you turn it on, and Folio tells you where images will be sent.
• Erase everything at any time from Settings.

Folio's readings are suggestions; check each expense before you submit a claim. Marking a claim "submitted" in Folio does not send it anywhere.

### Keywords (100)

`expense,receipt,claim,reimbursement,scanner,OCR,report,PDF,travel,business,tax,SST,MYR,offline`

### What's New (first version)

First release.

## App Review Information

**Sign-in required:** No. There are no accounts; every feature works without signing in.

**Contact:** [[first name, last name, phone, email]]

**Notes for review** (paste as-is, then attach the three images in `review-samples/`):

> Folio is a local-first expense claim app. There is no account or backend: receipts, expenses and claims are stored only on the device. No in-app purchases, ads or tracking.
>
> To review the core flow:
>
> 1. Save the attached sample receipts to Photos (they are fictional and marked "SAMPLE RECEIPT").
> 2. Scan tab → Choose from Library → select them. Folio copies each one and reads it with Apple's Vision text recognition on the device; the rows update as each finishes.
> 3. Expenses tab → open an expense → review or edit fields → Mark as ready.
> 4. Claims tab → create a claim → add the expenses → Export PDF (or CSV/Excel). The report opens in the share sheet.
> 5. Settings → Erase all data deletes everything on the device after a confirmation.
>
> Take Photo asks for camera permission; if declined, Folio explains how to allow it and Photos/Files still work. Choose from Library uses the system photo picker, so no photo-library permission is requested.
>
> Network use: (a) the third sample receipt is in SGD; Folio converts it to the default currency (MYR) by requesting the public ECB reference rate from api.frankfurter.dev (only a date and two currency codes are sent; it can be switched off in Settings). (b) "Allow online AI receipt extraction" in Settings is off by default and needs the user's own API key from an OpenAI-compatible provider; Folio shows the provider's host name before anything is sent. It is not needed to review the app, and without it all receipts are read on the device. (c) The app checks Expo's EAS Update service for updates at launch.
>
> Privacy Policy and Support open from the (i) button in the Settings tab.

Do **not** put a real API key in the review notes. If you want review to exercise online AI, create a temporary, spend-limited key for review only and revoke it after the review.

## App Privacy (nutrition label), proposed answers

These follow Apple's definition: data is "collected" when it leaves the device so that you or your third-party partners (SDKs you include) can access it beyond servicing the request in real time.

| Flow                                                                       | Leaves device to                                   | Proposed answer                                                                                                                                                                                                  |
| -------------------------------------------------------------------------- | -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Receipts, expenses, claims, name/organisation label                        | Nowhere (on device)                                | Not collected                                                                                                                                                                                                    |
| Exchange-rate lookup (date + 2 currency codes)                             | Frankfurter (public API, not an SDK in the app)    | Not collected: no personal data is sent, and Frankfurter states that it keeps no request logs                                                                                                                    |
| EAS Update check (random install ID, OS, app version/channel, IP)          | Expo (SDK in the app)                              | **Identifiers → Device ID**: App Functionality (and Analytics, if Expo's installation counts are treated as such); **not linked** to the user; **not used for tracking**. [[Confirm with Expo before answering]] |
| Online AI (receipt images), only if the user enables it with their own key | The user's own provider, under the user's contract | Owner decision. Not "collected" by you or your partners under Apple's definition. If you prefer to over-disclose: **User Content → Other User Content**, App Functionality, not linked, no tracking              |
| Crash reports through Apple (user opt-in)                                  | Apple                                              | Not part of your label (Apple's own collection)                                                                                                                                                                  |

**Tracking:** No. App Tracking Transparency is not needed: there is no tracking, IDFA, ad SDK or data broker.

## Age rating answers

All content descriptors: **None**. Medical or wellness: **No**. Chance-based activities: **No**. In-app controls (Parental Controls, Age Assurance): **No**. Capabilities: Unrestricted Web Access **No** (the only web pages are the privacy policy and support page in Safari View Controller), User-Generated Content **No** (nothing is shared between users), Social Media **No**, Messaging and Chat **No**, Advertising **No**. Expected rating: **4+**. Not for the Kids Category.

## Export compliance

`ITSAppUsesNonExemptEncryption` is `false` in `mobile/app.json`. Apple says to set it to NO only if the app, including its libraries, uses no encryption or only exempt encryption, and gives OS HTTPS (`URLSession`) as the typical exempt case. Folio's HTTPS comes from the Rust core's own TLS library (rustls), not the OS. Answer App Store Connect's encryption questions for that case, and change the key if your answers require documentation. **[owner / export-control advice]**

## Screenshots

Required: at least one iPhone screenshot at 1206 × 2622 (or 1179 × 2556) px, and, because `ios.supportsTablet` is `true`, at least one 13-inch iPad screenshot at 2064 × 2752 px (per Apple's screenshot specifications, November 2026). Draft captures from the iPhone 17 Pro and iPad Pro 13-inch simulators are in `screenshots/`. They use the fictional sample receipts only (2.3.9). Use them as-is or as a starting point; they must show the app in use, not a splash screen (2.3.3).
