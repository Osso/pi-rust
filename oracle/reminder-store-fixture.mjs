import { createReminder, listReminders } from "/home/osso/.worktrees/pi-delta-reminders-be6413897/packages/coding-agent/src/core/reminder-store.ts";

const [action, db] = process.argv.slice(2);
if (action === "create") {
  console.log(JSON.stringify(createReminder(db, { title: "from TypeScript", notes: "notes", projectPath: "/repo", dueAt: "2026-10-26T01:00:00+01:00" })));
} else if (action === "list") {
  console.log(JSON.stringify(listReminders(db, "all")));
} else {
  throw new Error(`Unknown fixture action: ${action}`);
}
