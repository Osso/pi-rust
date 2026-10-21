// Deterministic observation of the pinned Node runtime's scheduled delay.
import { createInterface } from "node:readline";

async function main() {
    for await (const line of createInterface({ input: process.stdin })) {
        const values = JSON.parse(line) as string[];
        const delays = values.map(value => {
            const timer = setTimeout(() => {}, Number(value));
            const delay = (timer as unknown as { _idleTimeout: number })._idleTimeout;
            clearTimeout(timer);
            return delay;
        });
        console.log(JSON.stringify(delays));
    }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
