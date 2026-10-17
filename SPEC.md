# Ticket tracker: spec (first pass)

## Data

The hub's `tickets` table: `id`, `customer_id` → customers, `assignee_id` → employees, `title`
(required, ≤ 240 characters), `body` (optional, ≤ 240), `status`, `created_at` (`yyyy-mm-ddThh:mm`).

Statuses, in board order: `open` → `in_progress` → `closed`. An unknown status is shown as Open.

## Board

- Three columns, newest first (by `created_at`, then `id`), each with a count.
- A card shows id, date, title, body, customer and assignee, and Edit / Delete / ← / → buttons.
- Filters: free text (title, body, customer name, or `#id`), assignee, customer. The header shows
  "shown of total".

## Changes

| Action | Mock snapshot | Shared database (served by the desktop program) |
|---|---|---|
| New ticket | added in memory | `POST /api/tickets` |
| Edit | in memory | `PATCH /api/tickets/{id}` with only the changed fields |
| ← / → | in memory | `PATCH` with `{"status": ...}` |
| Delete (after confirm) | in memory | `DELETE /api/tickets/{id}` |

Text is cleaned the way the hub's `textsafe` does, in the page and again before it is stored. An error
(400/404/409, same contract as the hub) is shown in the status bar with its message; the board keeps its
last good state.

## Not yet

Comments and history, due dates and priorities, auth, live updates from other clients.
