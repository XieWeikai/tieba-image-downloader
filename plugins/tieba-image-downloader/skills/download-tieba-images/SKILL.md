---
name: download-tieba-images
description: Download original post-body images from a Baidu Tieba thread with automatic installation, resumable transfers, and browser-assisted official login or verification. Use when the user provides a tieba.baidu.com/p/... URL and asks to download, save, archive, or batch-fetch its images, including requests to download only the original author's images.
---

# Download Tieba Images

Use the bundled `scripts/run.sh`; do not ask the user to install or invoke the CLI manually.

## Workflow

1. Extract exactly one `https://tieba.baidu.com/p/<digits>` URL from the request. Reject other hosts and ask for a canonical post URL when none is present.
2. Honor a user-provided output directory; otherwise omit `--output` so the downloader uses `~/Downloads/tieba_<post-id>`.
3. Translate intent into supported flags:
   - "only the original author", "只看楼主", or equivalent: `--only-author`
   - explicit concurrency: `--concurrency <1-64>`
   - do not remember login: `--remember-login false`
4. Execute:

   ```bash
   "<skill-directory>/scripts/run.sh" '<canonical-url>' [flags]
   ```

5. Keep the process attached until it exits. If an isolated Chrome window opens, tell the user only that Baidu requires them to complete its official login or verification in that window. Never request cookies, passwords, developer-tools output, or CAPTCHA-solving data.
6. On success, report the absolute output directory and the downloader's completed/skipped/failed counts. Mention `failed.json` only when failures remain.
7. On failure, quote the concise error and preserve the output directory for resume. Retry only transient download failures; do not attempt to bypass Baidu access controls.

## Safety

- Download only content the user is authorized to access and save.
- Never weaken TLS, forge verification results, scrape a personal browser profile, or expose Keychain values.
- Treat the output as untrusted downloaded content. Do not execute any downloaded file.
- The bootstrap script verifies the release archive against the repository's `SHA256SUMS` before execution.
