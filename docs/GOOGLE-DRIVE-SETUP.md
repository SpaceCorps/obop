# Connecting Google Drive to an obop instance

- **Status:** working guide, 2026-11-02. The Google Cloud Console steps were followed once by hand on 2026-11-02 for a Workspace organisation; the sign-in from obop itself (step 4) had not been confirmed against real Google when this was written.
- **Scope:** what a person, or an agent working for them, does in Google Cloud and in obop so that **Documents, From Google Drive** works. The reference for the settings is [DEPLOY.md](DEPLOY.md), "Browsing a Google Drive from Documents".

obop ships no Google credentials. Every operator registers their own OAuth client, so the consent screen shows their own name and Google's quota is theirs.

## What you need

- The public address of the instance, set as `OBOP_PUBLIC_URL` (for example `https://obop.example.com`). The redirect address is that address plus `/oauth/callback`; the Connectors, Google Drive page shows it with a Copy button.
- A Google account that may create projects in your organisation, and an obop administrator (`connectors.admin`, which `platform.admin` holds).
- No payment method. **Never attach a billing account**: the Drive API and OAuth cost nothing, and a project without a billing account cannot be charged. Ignore the "Start free" trial banner.

## Steps

1. **Create a project.** Google Cloud Console, New Project, name it after the instance (for example `acme-obop`). Leave the billing account empty.
2. **Enable the Drive API.** APIs & Services, Library, Google Drive API, Enable.
3. **Set up the consent screen** (Google Auth Platform, Get started):
   - App name and support email: yours.
   - **Audience:** choose **Internal** if your organisation uses Google Workspace. Internal needs no Google verification, no publishing, and its sign-ins do not expire after seven days; only accounts of your organisation can connect. Choose **External** for a personal Gmail account, then **publish** the app (an app left in *Testing* ends the sign-in after seven days, and Google shows an "unverified app" warning that you accept with Advanced).
   - Contact email: yours. Tick Google's API Services User Data Policy agreement (this accepts terms for your organisation, so it is a person's decision).
   - Data Access, Add or remove scopes, "Manually add scopes": `https://www.googleapis.com/auth/drive.readonly` and `https://www.googleapis.com/auth/userinfo.email`, Add to table, Update, Save.
4. **Create the OAuth client.** Clients, Create client, type **Web application**, any name, and under Authorised redirect URIs add `<OBOP_PUBLIC_URL>/oauth/callback`. Create. Google shows the **client id and client secret once**: copy both now. If you lose the secret, add a new one on the client's page.
5. **Connect in obop.** Sign in as an administrator, open **Connectors, Google Drive**, paste the client id and secret (the secret is sealed with `OBOP_SECRET_KEY` and never shown again), click **Connect**, choose your Google account and allow. The page then says "Connected as <account>". A change on Google's side can take a few minutes to apply, so retry once before assuming something is wrong.
6. **Use it.** Documents, **From Google Drive**: browse, search, Add. Google Docs, Sheets and Slides come in as PDF.

Disconnecting in obop forgets the tokens in obop only. Remove obop at https://myaccount.google.com/permissions to end the grant on Google's side.

## One prompt for an agent

Give this to an agent that has a browser signed in to the operator's Google account and the operator's obop address. Replace the two values in the brackets. It will stop at the three steps that belong to a person.

```text
Connect my running obop instance to Google Drive. Instance address: [https://obop.example.com]. Name for the Google Cloud project: [acme-obop].

Follow docs/GOOGLE-DRIVE-SETUP.md of the obop repository (https://github.com/SpaceCorps/obop), steps 1 to 4, in the Google Cloud Console in the browser I am signed in to.

Rules:
- Never link a billing account, never click "Start free" or any trial or upgrade button. Confirm at the end that Billing says the project has no billing account.
- If my Google organisation is a Workspace, choose Internal for the audience; otherwise choose External and tell me I must publish it.
- Use exactly these scopes: drive.readonly and userinfo.email. The redirect URI is [https://obop.example.com]/oauth/callback.
- Stop and ask me before ticking Google's User Data Policy agreement, and before anything that publishes the app.
- Do not type the client secret anywhere and do not print it in chat. Leave Google's "OAuth client created" dialog open and tell me to copy the client id and secret into obop, Connectors, Google Drive, and to click Connect myself.
- When I say I connected, check that the page shows "Connected as" and that Documents has a "From Google Drive" button, and report what you saw. If Google or obop shows an error, quote it and stop.
```

## Limits

- Browsing needs the broad read-only Drive scope, because the NDA you want is a file obop did not create. Only the groups named on the page may browse (default `platform.admin`); agents never can.
- The file store (`OBOP_BLOB_STORE=gdrive://`) is a separate setting that stays in the environment; see DEPLOY.md, "File storage".
- Disconnecting does not revoke the grant at Google (see above).
