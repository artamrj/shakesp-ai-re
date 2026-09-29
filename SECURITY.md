# Security policy

## Supported versions

Only the latest release receives fixes. shakespAIre updates itself from the menu bar icon, so
please make sure you run the newest version before reporting.

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately instead:

- Use GitHub's private reporting: **Security → Report a vulnerability** on
  [the repository](https://github.com/artamrj/shakesp-ai-re/security/advisories/new).

Include what you found, how to reproduce it, and the version and operating system. I will reply
as soon as I can, normally within a week, keep you updated, and credit you in the release notes if
you like.

## What is worth reporting

- The API key leaking (to a file, a log, the network, or another process).
- Anything that lets someone install an update that was not signed by the project's key.
- Selected text being sent anywhere other than the endpoint the user configured.
- Code execution or privilege problems in the app or its installers.

## Good to know

- shakespAIre has **no telemetry and no accounts**. The only other network request is the update
  check, which downloads a public file from GitHub Releases.
- The API key is stored in the operating system's keychain. Where none exists, it is kept in a
  file that only the current user can read.
- The macOS and Windows installers are not yet code-signed, so those platforms show warnings on
  first install. That is a known limitation, not a vulnerability.
