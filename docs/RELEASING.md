# Releasing shakespAIre

The bundle configuration is split by operating system and merged automatically by Tauri:

- macOS: `.app` and `.dmg`, Apple Silicon
- Windows: per-user NSIS `.exe` and WiX `.msi`
- Linux: AppImage, Debian `.deb`, and RPM

The checked-in icon source is `src-tauri/icons/icon.png`. Regenerate every platform size after
changing it with `npm run icons`. Do not manually resize only one output format.

## Local bundles

Run the command on the operating system being packaged:

```bash
npm run bundle:macos
npm run bundle:windows
npm run bundle:linux
```

Windows MSI uses WiX and must be built on Windows. Linux should be built on the oldest supported
Linux baseline; the release workflow uses Ubuntu 22.04 to avoid unnecessarily raising the glibc
requirement. The GitHub workflow builds Apple Silicon macOS and x64 Windows/Linux bundles.

Before tagging a release, keep these versions identical:

- `src-tauri/tauri.conf.json > version`
- `src-tauri/Cargo.toml > package.version`
- `package.json > version`

Push a tag such as `app-v0.2.0` to create a draft GitHub release.

## Platform signing

The default CI artifacts are suitable for internal testing, not a public trusted release.

### macOS

The macOS config uses ad-hoc signing (`signingIdentity: "-"`) so CI can create runnable Apple
Silicon artifacts without committing a certificate. For public distribution, override it with a
`Developer ID Application` identity through `APPLE_SIGNING_IDENTITY`, then notarize with either:

- `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_KEY_PATH`; or
- `APPLE_ID`, an app-specific `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.

CI also needs the exported `.p12` certificate imported into a temporary keychain. Store its base64
content and password as `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`; never commit it.

This app enables Tauri's macOS private API and uses private Liquid Glass APIs when available.
Treat the current build as direct-download software; private API use is not appropriate for a Mac
App Store submission. A Store build needs a separate config/code path that removes those APIs.

Accessibility approval is tied partly to the app identity. Stable Developer ID signing and the
unchanged bundle identifier `com.artamrj.shakespaire` reduce repeated permission prompts between
releases; ad-hoc development builds may prompt again.

### Windows

Unsigned installers run, but downloads can trigger Microsoft SmartScreen. For public releases,
use an EV/OV certificate, Azure Artifact Signing, or a configured `bundle.windows.signCommand`.
Timestamp every signature so it remains valid after the certificate expires. Keep the checked-in
WiX `upgradeCode` unchanged or Windows will install upgrades as a second application.

### Linux

Signing is optional. AppImage supports an embedded GPG signature with `SIGN=1`, `SIGN_KEY`, and
`APPIMAGETOOL_SIGN_PASSPHRASE`, but users must verify it explicitly. Repository-hosted DEB/RPM
packages should instead be signed through the repository metadata and package-publishing process.

Linux text replacement still needs `wtype` on Wayland or `xdotool` on X11. These are intentionally
not hard dependencies because users need only one of them and package names differ by distribution.

## Enabling secure auto-update

Updater output is intentionally disabled until a permanent signing key exists. Tauri does not
allow unsigned updates.

1. Generate and back up a permanent key outside the repository:

   ```bash
   npm run tauri signer generate -- -w src-tauri/shakespaire.key
   ```

2. Add the updater plugin with `npm run tauri add updater`.
3. Set `bundle.createUpdaterArtifacts` to `true`.
4. Add `plugins.updater.pubkey` containing the public key text—not its path—and configure the
   HTTPS endpoint. For GitHub Releases, the static endpoint can be:
   `https://github.com/artamrj/ShakespAIre/releases/latest/download/latest.json`.
5. Add `TAURI_SIGNING_PRIVATE_KEY` and, if used, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub
   Actions secrets and expose them only to the release build step.
6. Add an explicit update check/download/install UI. Do not silently restart while the user is
   proofreading or replacing text.

Losing the updater private key prevents publishing trusted updates to existing installations.
Rotating it requires a migration release signed by the old key, so keep an encrypted offline backup.
