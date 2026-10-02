# Security Policy

Unarc takes security and zero-trust execution boundaries seriously. This document outlines our vulnerability disclosure policy and reporting procedures.

---

## Supported Versions

Only the latest release version receives active security patches.

| Version | Supported          | Notes |
| ------- | ------------------ | ----- |
| 0.2.x   | :white_check_mark: | Current active release branch |
| < 0.2.0 | :x:                | Superseded; upgrade to latest release |

---

## Reporting a Security Vulnerability

If you believe you have discovered a security vulnerability in Unarc—including path traversal escapes, sandbox circumvention, process isolation failures, cryptographic update bypasses, or secret leakage—please report it responsibly through private channels.

**Do NOT report security vulnerabilities via public GitHub issues, discussions, or pull requests.**

### How to Report

Please report sensitive security issues via email to:

`SECURITY_CONTACT_EMAIL: <SECURITY_EMAIL_PLACEHOLDER>`

*(Repository owners: replace `<SECURITY_EMAIL_PLACEHOLDER>` with your configured security contact address or GitHub Private Vulnerability Reporting link).*

### What to Include in Your Report

To help us triage and resolve the issue quickly, please include:
1. **Summary**: A clear description of the vulnerability and its potential security impact.
2. **Component**: The affected subsystem (e.g. path sanitizer, subprocess sandboxing, updater, bundled engine integration).
3. **Environment**:
   - Unarc version (`unarc version`)
   - Platform / Architecture (`unarc doctor`)
   - Operating system (macOS version or Linux distribution / Docker runtime)
4. **Reproduction Steps / PoC**: Minimal, reproducible steps or test archive files demonstrating the issue.
5. **Mitigations**: Any workarounds or mitigations you have identified.

---

## Coordinated Vulnerability Disclosure

- We will acknowledge receipt of your report within 48 business hours.
- We will provide an assessment and work on a fix in a private branch.
- Once a fix is verified and packaged into an official release, a public disclosure and security advisory will be coordinated.
- Please do not disclose vulnerabilities publicly prior to the release of an official security patch.
