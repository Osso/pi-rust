import { pathToFileURL } from "node:url";

type RequestId = string | number | null;

type Skill = {
	name: string;
	description: string;
	filePath: string;
	baseDir: string;
	sourceInfo: Record<string, unknown>;
	disableModelInvocation: boolean;
};

type SkillsApi = {
	loadSkillsFromDir(options: { dir: string; source: string }): {
		skills: Skill[];
		diagnostics: unknown[];
	};
	formatSkillsForPrompt(skills: Skill[]): string;
	loadSkills(options: {
		cwd: string;
		agentDir: string;
		skillPaths: string[];
		includeDefaults: boolean;
	}): { skills: Skill[]; diagnostics: unknown[] };
};

type Request =
	| { id: RequestId; operation: "load_dir"; dir: string; source: string }
	| { id: RequestId; operation: "format"; skills: Skill[] }
	| {
			id: RequestId;
			operation: "load";
			cwd: string;
			agentDir: string;
			skillPaths: string[];
			includeDefaults: boolean;
	  };

type Response =
	| { id: RequestId; ok: true; output: unknown }
	| { id: RequestId; ok: false; error: { name: string; message: string } };

function normalizeError(error: unknown): { name: string; message: string } {
	if (error instanceof Error) {
		return { name: error.name, message: error.message };
	}
	return { name: "UnknownError", message: String(error) };
}

function execute(request: Request, api: SkillsApi): unknown {
	switch (request.operation) {
		case "load_dir":
			return api.loadSkillsFromDir({ dir: request.dir, source: request.source });
		case "format":
			return { prompt: api.formatSkillsForPrompt(request.skills) };
		case "load": {
			const result = api.loadSkills({
				cwd: request.cwd,
				agentDir: request.agentDir,
				skillPaths: request.skillPaths,
				includeDefaults: request.includeDefaults,
			});
			// Multi-source discovery order depends on directory enumeration; compare sorted.
			result.skills.sort((left, right) => (left.filePath < right.filePath ? -1 : left.filePath > right.filePath ? 1 : 0));
			return result;
		}
	}
}

function executeRequestLine(line: string, api: SkillsApi): void {
	let id: RequestId = null;
	try {
		const request = JSON.parse(line) as Request;
		id = request.id;
		const response: Response = { id, ok: true, output: execute(request, api) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	} catch (error) {
		const response: Response = { id, ok: false, error: normalizeError(error) };
		process.stdout.write(`${JSON.stringify(response)}\n`);
	}
}

function serveRequests(api: SkillsApi): void {
	let buffer = "";
	process.stdin.on("data", (chunk: string | Buffer) => {
		buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8");
		while (true) {
			const lineFeed = buffer.indexOf("\n");
			if (lineFeed === -1) return;
			executeRequestLine(buffer.slice(0, lineFeed), api);
			buffer = buffer.slice(lineFeed + 1);
		}
	});
}

async function main(): Promise<void> {
	const sourceModulePath = process.argv[2];
	if (sourceModulePath === undefined) {
		throw new Error("skills oracle requires a source module path");
	}
	const api = (await import(pathToFileURL(sourceModulePath).href)) as SkillsApi;
	serveRequests(api);
}

void main().catch((error: unknown) => {
	process.stderr.write(`skills oracle startup failed: ${normalizeError(error).message}\n`);
	process.exitCode = 1;
});
