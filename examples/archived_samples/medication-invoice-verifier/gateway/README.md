# Gateway Skeleton (Sinatra + Sidekiq)

This gateway receives invoice events and enqueues verification jobs.

## Endpoints
- `GET /health`
- `POST /events/medication_invoice_submitted`

## Why this exists
- Provides a simple node-server-like ingress for external systems.
- Decouples request handling from policy evaluation via async queue.
- Can evolve to invoke Devlish parser/executor once process grammar support is complete.

## Not run in this task
Files are scaffolded only; no server, queue, or worker was executed.
