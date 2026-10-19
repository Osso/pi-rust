/**
 * Oracle fixture for the parent-death watchdog tests.
 *
 * It never reads stdin and keeps a timer alive, so it survives stdin EOF. Only
 * `oracle/parent-watchdog.mjs` can terminate it when the test process or its
 * tsx wrapper disappears.
 */
setInterval(() => {}, 1_000);
