# Testing Runbooks

These examples are meant to be readable QA workflows that help design and review browser-verification runbooks.

The current Devlish language is strongest at deterministic intake, routing, and decision logic. The actual browser-driving work still happens in the shared Star Playwright harness at `/Users/admin/code/star-browser-verify`.

Use these examples when you want to:

- document when a request should use the unattended Playwright suite
- document when a request should switch to Codex Playwright MCP for interactive debugging
- design a repeatable testing runbook before turning it into code
