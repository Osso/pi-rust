import { parseReminderDueAt, formatReminderDueAt, isReminderDueToday } from "/home/osso/.worktrees/pi-delta-reminders-be6413897/packages/coding-agent/src/core/reminder-time.ts";

const inputs = ["2026-07-21T14:30", "2026-01-21 14:30:12", "2026-03-08T02:30", "2026-11-01T01:30", "2026-10-04T02:15", "2026-04-05T01:45", "2011-12-30T12:00", "2026-07-21T14:30:12.1+05:45", "0099-01-02T03:04Z", "0000-02-29T03:04Z", "2026-02-30T12:00Z", "2026-07-21T14:30+05:60", "2026-07-21T14:30+24:00", "2026-07-21 14:30Z", "2026-07-21T14:30:00.123", "bad", "2026-07-21T14:30Z\n"];
const formats = ["2026-07-21T19:30:12.000Z", "2026-01-21T20:30:12.000Z", "2026-11-01T06:30:00.000Z", "2026-11-01T07:30:00.000Z", "2026-07-21T14:30", "bad"];
const now = Date.parse("2026-07-22T01:00:00Z");
const due = ["2020-01-01T00:00Z", "2026-07-22T04:59:59.999Z", "2026-07-22T05:00:00Z", "bad"];
const cases = [...inputs.map(input => ({op:"parse", input})), ...formats.map(input => ({op:"format", input})), ...due.map(input => ({op:"due", input, now})), ...["2026-03-08T06:00:00Z", "2026-11-01T05:00:00Z"].flatMap(clock => ["2026-03-09T04:59:59.999Z", "2026-03-09T05:00:00Z", "2026-11-02T05:59:59.999Z", "2026-11-02T06:00:00Z"].map(input => ({op:"due", input, now:Date.parse(clock)})))];
const result = cases.map(c => {
    try { return {...c, result:{value: c.op === "parse" ? parseReminderDueAt(c.input) : c.op === "format" ? formatReminderDueAt(c.input) : isReminderDueToday(c.input, new Date(c.now!))}}; }
    catch (e) { return {...c, result:{error:(e as Error).message}}; }
});
console.log(JSON.stringify(result));
