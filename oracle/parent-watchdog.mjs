/**
 * Preloaded into TypeScript oracles and mode harnesses via `NODE_OPTIONS`.
 *
 * Test children must not outlive the test process. When the test binary dies
 * (including `SIGKILL`, where no Rust `Drop` runs), this watchdog notices that
 * the recorded parent PID is gone and takes down its process group. It also
 * exits when its own parent (the tsx wrapper) disappears, which covers the
 * normal `Drop` path where the wrapper is killed before the Node child.
 *
 * The oracle is spawned as a process-group leader, so the group kill also
 * terminates the Node child and the esbuild service when the wrapper notices
 * first. For non-leader children the negative pid has no group and the signal
 * is ignored; those children then exit on the parent change.
 *
 * `PI_ORACLE_PARENT_PID` is set by the Rust oracle support code; when absent
 * the module does nothing.
 */
const parentPid = Number(process.env.PI_ORACLE_PARENT_PID);
const originalPpid = process.ppid;

function isAlive(pid) {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error.code === "EPERM";
	}
}

function shouldExit() {
	if (Number.isInteger(parentPid) && parentPid > 0 && !isAlive(parentPid)) {
		return true;
	}
	return process.ppid !== originalPpid;
}

function exitWithGroup() {
	try {
		process.kill(-process.pid, "SIGKILL");
	} catch {
		// No such group: this process is not a group leader.
	}
	process.exit(0);
}

if (Number.isInteger(parentPid) && parentPid > 0) {
	if (shouldExit()) {
		exitWithGroup();
	}
	const timer = setInterval(() => {
		if (shouldExit()) {
			exitWithGroup();
		}
	}, 250);
	timer.unref();
}
