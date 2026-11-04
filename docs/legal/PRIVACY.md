# Folio privacy policy (DRAFT)

> **Draft, not reviewed by a lawyer.** It describes what Folio 0.1.0 does, checked against the source code on 5 November 2026. Replace every `[[…]]` placeholder and have it reviewed before you publish it. If the Personal Data Protection Act 2010 applies to you as Folio's publisher, section 7(3) requires the notice in both Bahasa Malaysia and English (see [the launch audit](../LAUNCH_AUDIT.md)).

**Last updated:** [[date of publication]]

## Who publishes Folio

Folio is published by [[legal name of the person or company]] ([[SSM registration number, if a business]]), [[postal address]]. Questions about this policy: [[contact email]].

## The short version

- Folio keeps your receipts, expenses, claims and settings **on your own device**. We, Folio's publisher, do not receive them. There is no Folio account, cloud sync, advertising, analytics or tracking.
- Data leaves your device only in the cases listed under [When data leaves your device](#when-data-leaves-your-device): exchange-rate look-ups (on by default, can be turned off), online AI receipt reading (off by default), and app update checks in the iPhone app.
- You can delete a single expense, or erase everything with **Settings → Erase all data**.

## What Folio stores on your device

| Data                                                                                        | Why                                      | Where                                                                                                                 |
| ------------------------------------------------------------------------------------------- | ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Receipt files you import (photos, PDFs) and previews made from them                         | To show, read and attach your receipts   | Folio's data folder (shown in Settings)                                                                               |
| Expense details: merchant, premises, date, amounts, tax, currency, category, your notes     | To prepare expense claims                | Local database in the data folder                                                                                     |
| Text read from receipts by OCR or AI, and how confident the reading was                     | To let you check and correct readings    | Local database                                                                                                        |
| Claims, and the PDF, CSV and Excel reports you generate                                     | To submit and keep your claims           | Local database and the data folder's `exports` folder                                                                 |
| An internal activity log (record IDs, event types, which fields changed, file fingerprints) | To keep records consistent and traceable | Local database. It does not contain receipt text or amounts                                                           |
| Your preferences, including the name and organisation label you enter                       | Printed on your claim reports            | Local database                                                                                                        |
| Exchange rates already downloaded                                                           | So each rate is fetched only once        | Local database                                                                                                        |
| The AI provider API key, if you add one                                                     | To use your AI provider                  | Your operating system's credential vault (Keychain on Apple devices, Credential Manager on Windows), not the database |

Receipts often show other people's information, such as a cashier's name, or a card's last digits. Only import receipts you are entitled to use.

**How long it is kept:** until you delete it. Folio does not delete records automatically.

**Security:** Folio restricts its data folder to your user account on macOS, and uses your operating system's protections on Windows and iOS. It does not add its own encryption to the database or receipt files. Use your device's encryption (for example FileVault, BitLocker or a device passcode).

## When data leaves your device

### Exchange rates (on by default)

To convert a foreign-currency receipt into your default currency, Folio asks [Frankfurter](https://frankfurter.dev) for the European Central Bank's published rate. Each request contains a date and two currency codes, never receipt content. As with any internet request, Frankfurter (and its network provider, Cloudflare) can see your IP address. Frankfurter states that its API does not log personal data, IP addresses or request URLs. Turn this off under **Settings → Convert foreign receipts to my default currency**.

### Online AI receipt reading (off by default)

If you switch on **Allow online AI receipt extraction** and add your own API key, Folio sends images of each receipt you import to the AI provider whose address you enter (for example OpenAI or Google). The request uses your key, so it is governed by your agreement with that provider, including where it processes the images and how long it keeps them. Folio does not send your name or organisation label, and we do not receive a copy. Your provider may process the images outside Malaysia.

### App updates (iPhone app only)

When the iPhone app starts, it checks for updates with Expo's EAS Update service, run by 650 Industries, Inc. ("Expo"). The check sends a random identifier created for your installation (not derived from your device or account), your device's operating system, the app's version and update channel, and, as with any internet request, your IP address. Expo uses this to deliver updates and to count installations. Expo may process it in the United States. See [Expo's privacy policy](https://expo.dev/privacy). [[Confirm with Expo what it retains and for how long, then update this section.]]

The desktop app does not check for updates or contact us.

### App stores and crash reports

If you download Folio from an app store, that store handles your download and any payment under its own privacy policy. If you allow your iPhone to share analytics with app developers, Apple may send us crash reports. [[Remove this paragraph if not distributed through an app store.]]

### Files you export or share

Reports and CSV or Excel files you export, copy to another folder, or share (for example by email or the iPhone share sheet) leave Folio's control. Folio does not track them.

## Deleting your data

- **One expense:** open it and choose **Delete expense** (or select several in the expense list). Its receipt file is deleted too. The activity log keeps the deleted record's ID and the receipt's fingerprint (a hash), not its content. Expenses in a submitted or archived claim must be reopened first.
- **Everything:** **Settings → Erase all data** permanently deletes every receipt, expense, claim, report, activity-log entry, preference and downloaded exchange rate in Folio's data folder, and removes the stored API key. The database file is rewritten so erased records do not remain in its unused space.
- **Uninstalling** the iPhone app removes its data folder, but iOS may keep a stored API key in the Keychain; remove the key in Settings (or erase all data) first. On a desktop computer, uninstalling may leave the data folder in place; erase the data first or delete the folder shown in Settings.

Erasing does not reach copies outside Folio's data folder: reports you saved to another folder or shared, device or computer backups made before you erased (for example iCloud, iTunes/Finder, Time Machine or Windows backups) until those backups are replaced, and images already sent to your AI provider (ask that provider to delete them).

### Third parties and how they protect data

Expo is the only service that receives data from Folio on our behalf. [[Before publishing, confirm that Expo's terms (for example a data processing agreement) protect this data at least as well as this policy. App Store Review Guideline 5.1.1(i) requires this policy to confirm it.]] Frankfurter receives no personal data beyond the IP address that every internet connection reveals. Your AI provider is a service you choose and contract with yourself; Folio contacts it only on your instruction and with your key, so its own terms, not ours, decide how it protects your images.

## Camera, photos and files (iPhone)

Folio asks for camera access only when you choose **Take Photo**. **Choose from Library** uses Apple's photo picker, which gives Folio only the photos you select, so Folio does not ask for access to your photo library. **Choose Files** gives Folio only the files you pick. On iPhone, receipts are read with Apple's on-device text recognition; nothing is sent online unless you turn on online AI reading.

## Withdrawing consent

You can stop each flow at any time: switch off **Allow online AI receipt extraction** (and remove the API key), switch off **Convert foreign receipts to my default currency**, and turn off camera access in iOS Settings → Folio. Switching a feature off stops future sending; it does not recall data already sent.

## Your choices and rights

Because your Folio data stays on your device, you can see, correct, export and delete it yourself in the app at any time. For data held by Frankfurter, your AI provider or Expo, contact them directly. For any question or complaint about this policy, contact us at [[contact email]]. [[Lawyer to confirm whether, and which, rights under the Personal Data Protection Act 2010 apply, and how requests are handled.]]

## Children

Folio is a tool for managing work and personal expenses. It is not directed at children, has no accounts and asks no one for their age.

## Changes to this policy

When Folio's data handling changes, we will update this policy and its "Last updated" date before releasing that version. [[Say how users will be told, for example release notes.]]

## Contact

[[Legal name]], [[postal address]], [[contact email]].
