# Security Policy

## Supported Versions

Security fixes are applied to the latest published release and the `main`
branch.

| Version        | Supported |
| -------------- | --------- |
| Latest release | Yes       |
| `main`         | Yes       |
| Older releases | No        |

## Reporting a Vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's
[private vulnerability reporting form](https://github.com/namannn04/Veronica/security/advisories/new)
so the report and any follow-up remain confidential.

Include the affected version, how Veronica was installed, your Ubuntu and GNOME
Shell versions, reproduction steps, expected and observed behavior, potential
impact, and any suggested mitigation. Remove credentials, agent history, and
personal data from the report.

The maintainers aim to acknowledge a complete report within three business days
and provide an initial assessment within seven business days. Remediation and
disclosure timing depend on severity, complexity, and release availability. The
reporter will receive updates when the assessment changes materially.

Please allow time for a fix and coordinated disclosure before publishing
details. A GitHub security advisory will be used to request a CVE when one is
warranted.

## Safe Harbor

Good-faith research that follows this policy is considered authorized. Avoid
privacy violations, service disruption, destructive testing, social
engineering, and access beyond what is necessary to demonstrate the issue. Stop
testing and report immediately if you encounter sensitive data. The project will
not pursue action against researchers who follow these rules and make a
reasonable effort to avoid harm.

## Security Model

Veronica controls the computer it runs on. It reads local coding-agent history,
runs package managers through `pkexec` only after an explicit confirmation, and
talks to its GNOME Shell extension and the `vr` command over a socket in the
user's runtime directory. It has no account and no telemetry; network access
happens only for features that need it or that the user runs.

Release packages are built by GitHub Actions from a tagged commit and published
with SHA-256 checksums. The install script verifies the checksum before
installing, and the apt and Flatpak repositories are signed.

Reports are especially useful when they show command or argument injection,
another local user or a website reaching the IPC socket, privilege escalation
through a package operation, agent history or credentials leaving the machine,
writes outside Veronica's own directories, or a way to install an unverified
package.
