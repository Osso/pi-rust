type Request = {
	id: number;
	text: string;
};

type NormalizationResult = {
	nfc: string;
	nfd: string;
	nfkc: string;
	nfkd: string;
};

function handleLine(line: string): void {
	if (line.length === 0) {
		return;
	}
	const request = JSON.parse(line) as Request;
	const result: NormalizationResult = {
		nfc: request.text.normalize("NFC"),
		nfd: request.text.normalize("NFD"),
		nfkc: request.text.normalize("NFKC"),
		nfkd: request.text.normalize("NFKD"),
	};
	process.stdout.write(`${JSON.stringify({ id: request.id, result })}\n`);
}

let buffer = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk: string) => {
	buffer += chunk;
	let newline = buffer.indexOf("\n");
	while (newline >= 0) {
		const line = buffer.slice(0, newline);
		buffer = buffer.slice(newline + 1);
		handleLine(line);
		newline = buffer.indexOf("\n");
	}
});
