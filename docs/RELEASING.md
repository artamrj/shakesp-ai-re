# Releasing shakespAIre

The bundle configuration is split by operating system and merged automatically by Tauri:

- macOS: `.app` and `.dmg`, Apple Silicon
- Windows: per-user NSIS `.exe` and WiX `.msi`
- Linux: AppImage, Debian `.deb`, and RPM

The checked-in icon source is `assets/branding/shakesp-ai-re-icon-master.png`. Regenerate every
platform size after changing it with `npm run icons`. Do not manually resize only one output format.

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

## How a release happens

Releases are automatic; you never edit a version or create a tag by hand.

1. Merge pull requests into `main` with [Conventional Commit](https://www.conventionalcommits.org)
   titles (`feat: …`, `fix: …`, `perf: …`). The pull-request title check enforces this, and the
   squash-merged title becomes the commit message.
2. On every push to `main`, the `Release` workflow (release-please) opens or updates a
   **Release pull request**. It bumps the version in `package.json`, `package-lock.json`,
   `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, and writes
   `CHANGELOG.md`. While the version is below 1.0, `feat` bumps the minor and `fix` the patch.
   `docs`, `ci`, `chore`, `refactor`, `build`, `test` and `style` commits do not trigger a release.
3. Merge that pull request when you want to ship. The workflow then creates the `vX.Y.Z` tag and a
   draft GitHub release with the changelog as its notes, builds the macOS, Windows and Linux
   installers (`release.yml`) and attaches them to the draft.
4. The last job, `Publish release`, runs in the `release` environment. If you add yourself as a
   required reviewer there, it waits for one approval click before the draft becomes public.

To rebuild the installers for an existing tag, run the `Build installers` workflow manually and
enter the tag.

### One-time repository setup

- **Environment:** Settings > Environments > New environment `release`, then add yourself under
  *Required reviewers* (this is the final approval gate).
- **Token (recommended):** create a fine-grained personal access token limited to this repository
  with *Contents* and *Pull requests* read/write, and save it as the secret `RELEASE_PLEASE_TOKEN`.
  Pull requests opened with the default token do not trigger CI, so without it the Release pull
  request has no `CI` check; you would have to merge it as an admin using the ruleset bypass.
- **Workflow permissions:** Settings > Actions > General > *Allow GitHub Actions to create and
  approve pull requests* must be enabled.

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
unchanged bundle identifier `com.artamrj.shakesp-ai-re` reduce repeated permission prompts between
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
   npm run tauri signer generate -- -w src-tauri/shakesp-ai-re.key
   ```

2. Add the updater plugin with `npm run tauri add updater`.
3. Set `bundle.createUpdaterArtifacts` to `true`.
4. Add `plugins.updater.pubkey` containing the public key text—not its path—and configure the
   HTTPS endpoint. For GitHub Releases, the static endpoint can be:
   `https://github.com/artamrj/shakesp-ai-re/releases/latest/download/latest.json`.
5. Add `TAURI_SIGNING_PRIVATE_KEY` and, if used, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as GitHub
   Actions secrets and expose them only to the release build step.
6. Add an explicit update check/download/install UI. Do not silently restart while the user is
   proofreading or replacing text.

Losing the updater private key prevents publishing trusted updates to existing installations.
Rotating it requires a migration release signed by the old key, so keep an encrypted offline backup.
