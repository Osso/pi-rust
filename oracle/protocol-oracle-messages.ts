type RequestId = string | number | null;

export type CommandRequest = {
	command: string;
	cwd?: string;
	name?: string;
	model?: {
		provider: string;
		id: string;
	};
	thinkingLevel?: string;
	sessionId?: string;
	text?: string;
};

export type ClientHelloRequest = {
	type: "hello";
	version: number;
};

export type RequestEnvelopeRequest = {
	type: "request";
	id: string;
	request: CommandRequest;
};

export type ClientMessageRequest = ClientHelloRequest | RequestEnvelopeRequest;

type MessageValidationValue = CommandRequest | ClientMessageRequest;

export type MessageValidationRequest =
	| {
			id: RequestId;
			operation: "validate_command";
			value: CommandRequest;
		}
	| {
			id: RequestId;
			operation: "validate_client_hello";
			value: ClientHelloRequest;
		}
	| {
			id: RequestId;
			operation: "validate_request_envelope";
			value: RequestEnvelopeRequest;
		}
	| {
			id: RequestId;
			operation: "validate_client_message";
			value: ClientMessageRequest;
		};

export type MessageSchemaApi = {
	CommandSchema: unknown;
	ClientHelloSchema: unknown;
	RequestEnvelopeSchema: unknown;
	ClientMessageSchema: unknown;
};

type TypeboxApi = {
	Check(schema: unknown, value: unknown): boolean;
};

export type MessageValidationResponse = {
	id: RequestId;
	ok: true;
	output:
		| { valid: false }
		| {
				valid: true;
				value: MessageValidationValue;
		  };
};

export function executeMessageValidation(
	request: MessageValidationRequest,
	schema: MessageSchemaApi,
	typebox: TypeboxApi,
): MessageValidationResponse {
	const selectedSchema = selectMessageSchema(request.operation, schema);
	if (!typebox.Check(selectedSchema, request.value)) {
		return { id: request.id, ok: true, output: { valid: false } };
	}
	return { id: request.id, ok: true, output: { valid: true, value: request.value } };
}

function selectMessageSchema(operation: MessageValidationRequest["operation"], schema: MessageSchemaApi): unknown {
	switch (operation) {
		case "validate_command":
			return schema.CommandSchema;
		case "validate_client_hello":
			return schema.ClientHelloSchema;
		case "validate_request_envelope":
			return schema.RequestEnvelopeSchema;
		case "validate_client_message":
			return schema.ClientMessageSchema;
	}
}
