# Status indicators and countdown

Frozen `4f9e6f8a4`; [status proof](../porting/traceability/interactive-status-indicator.md), [countdown proof](../porting/traceability/countdown-timer.md).

- Status kinds use frozen themed labels and always force empty spinner frames regardless of supplied working frames. Idle renders two full-width space rows. No status advertises an input handler.
- Subclasses retain base message-update and kind access. Component trait disposal stops existing loaders/countdowns, including scoped-chat retirement.
- Retry attempt/maxAttempts/delayMs and remaining countdown seconds accept every JavaScript number through f64. Initial seconds are Math.ceil(delayMs/1000); each one-second tick subtracts one, updates text, requests render, and only then retires/expires when seconds <= 0.
- Preserve negative zero in callback values, JavaScript NaN/Infinity arithmetic and comparisons, fractional counters, and exact JS numeric label formatting. Do not clamp, saturate or truncate timer values.
