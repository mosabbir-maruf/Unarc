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

If you believe you have discovered a security vulnerability in Unarc—including path traversal escapes, sandbox circumvention, process isolation failures, cryptographic update bypasses, or secret leakage—please report it responsibly through the repository's private security reporting channel.

**Do NOT disclose or report sensitive vulnerabilities through public issues, discussions, or pull requests.** Public disclosure puts users and systems at immediate risk before a fix can be prepared and released.

### How to Report

All vulnerability reports must be submitted exclusively through GitHub's Private Vulnerability Reporting mechanism:

1. Navigate to the repository page on GitHub.
2. Click on the **Security** tab.
3. Under the **Reporting** section in the left sidebar, click **Report a vulnerability** (or **Advisories** -> **New draft security advisory**).
4. Fill out the advisory form with the details outlined below and submit.

This opens a private advisory draft visible only to repository maintainers, where we can collaborate directly with you on verifying the issue and preparing a fix.

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
