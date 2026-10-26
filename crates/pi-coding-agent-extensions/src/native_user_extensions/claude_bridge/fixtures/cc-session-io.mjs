// src/session.ts
import { randomUUID } from "crypto";
import { mkdirSync, writeFileSync, appendFileSync, existsSync, rmSync } from "fs";
import { dirname } from "path";

// src/jsonl.ts
import { readFileSync } from "fs";
function parseJsonl(content) {
  return content.split("\n").filter((line) => line.trim()).map(parseRecord);
}
function parseJsonlFile(path) {
  return parseJsonl(readFileSync(path, "utf-8"));
}
function parseRecord(line) {
  const raw = JSON.parse(line);
  if (raw.type === "user") return raw;
  if (raw.type === "assistant") return raw;
  return raw;
}
function serializeRecord(record) {
  return JSON.stringify(record);
}
function serializeJsonl(records) {
  return records.map(serializeRecord).join("\n") + "\n";
}

// src/paths.ts
import { realpathSync } from "fs";
import { readdir, stat } from "fs/promises";
import { homedir } from "os";
import { join } from "path";
var MAX_SANITIZED_LENGTH = 200;
function getClaudeDir(claudeDir) {
  return claudeDir ?? process.env.CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude");
}
function normalizeProjectPath(projectPath) {
  try {
    return realpathSync(projectPath).normalize("NFC");
  } catch {
    return projectPath.normalize("NFC");
  }
}
function projectPathToHash(projectPath) {
  const sanitized = projectPath.replace(/[^a-zA-Z0-9]/g, "-");
  if (sanitized.length <= MAX_SANITIZED_LENGTH) return sanitized;
  let h = 0;
  for (let i = 0; i < projectPath.length; i++)
    h = (h << 5) - h + projectPath.charCodeAt(i) | 0;
  return `${sanitized.slice(0, MAX_SANITIZED_LENGTH)}-${Math.abs(h).toString(36)}`;
}
function getProjectDir(projectPath, claudeDir) {
  return join(getClaudeDir(claudeDir), "projects", projectPathToHash(normalizeProjectPath(projectPath)));
}
function getSessionPath(sessionId, projectPath, claudeDir) {
  return join(getProjectDir(projectPath, claudeDir), `${sessionId}.jsonl`);
}

// src/repair.ts
function repairToolPairing(messages) {
  return repairWithOrigin(messages).messages;
}
function repairWithOrigin(messages) {
  const result = [];
  const origin = [];
  let pending = null;
  const synthetic = (id) => ({
    type: "tool_result",
    tool_use_id: id,
    content: "[no tool result recorded]",
    is_error: true
  });
  const flushPending = () => {
    if (pending && pending.size > 0) {
      result.push({ role: "user", content: [...pending].map(synthetic) });
      origin.push(null);
    }
    pending = null;
  };
  for (const [index, msg] of messages.entries()) {
    if (msg.role === "assistant") {
      flushPending();
      const ids = /* @__PURE__ */ new Set();
      if (Array.isArray(msg.content)) {
        for (const b of msg.content) {
          if (b.type === "tool_use" && typeof b.id === "string") ids.add(b.id);
        }
      }
      result.push(msg);
      origin.push(index);
      pending = ids.size > 0 ? ids : null;
      continue;
    }
    const blocks = Array.isArray(msg.content) ? msg.content : null;
    const hasToolResults = blocks?.some((b) => b.type === "tool_result") ?? false;
    if (!pending && !hasToolResults) {
      result.push(msg);
      origin.push(index);
      continue;
    }
    const input = blocks ?? (typeof msg.content === "string" && msg.content ? [{ type: "text", text: msg.content }] : []);
    const provided = /* @__PURE__ */ new Set();
    const kept = input.filter((b) => {
      if (b.type !== "tool_result") return true;
      if (pending?.has(b.tool_use_id)) {
        provided.add(b.tool_use_id);
        return true;
      }
      return false;
    });
    if (pending) {
      const missing = [...pending].filter((id) => !provided.has(id)).map(synthetic);
      kept.unshift(...missing);
      pending = null;
    }
    if (kept.length === 0) {
      if (result.length === 0) {
        result.push({ role: "user", content: [{ type: "text", text: "[orphaned tool result removed]" }] });
        origin.push(null);
      }
      continue;
    }
    result.push({ ...msg, content: kept });
    origin.push(index);
  }
  flushPending();
  return { messages: result, origin };
}

// src/session.ts
var ADJECTIVES = [
  "ancient",
  "bold",
  "bright",
  "calm",
  "clever",
  "cool",
  "crimson",
  "daring",
  "eager",
  "fast",
  "fierce",
  "gentle",
  "golden",
  "happy",
  "hidden",
  "iron",
  "keen",
  "lively",
  "mighty",
  "nimble",
  "pale",
  "proud",
  "quick",
  "rapid",
  "sharp",
  "silent",
  "smooth",
  "steady",
  "swift",
  "vivid",
  "warm",
  "wild"
];
var NOUNS = [
  "arrow",
  "badge",
  "beacon",
  "blade",
  "brook",
  "castle",
  "cedar",
  "cloud",
  "comet",
  "crest",
  "dawn",
  "drift",
  "eagle",
  "ember",
  "falcon",
  "flame",
  "forge",
  "gale",
  "grove",
  "harbor",
  "hawk",
  "jade",
  "kettle",
  "lance",
  "maple",
  "marsh",
  "needle",
  "oak",
  "pearl",
  "pine",
  "ridge",
  "river",
  "rune",
  "sage",
  "shade",
  "spark",
  "stone",
  "thorn",
  "tower",
  "wave"
];
function pick(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}
function generateSlug() {
  return `${pick(ADJECTIVES)}-${pick(ADJECTIVES)}-${pick(NOUNS)}`;
}
function syntheticMessageId() {
  return `msg_syn_${randomUUID().replace(/-/g, "").slice(0, 24)}`;
}
function syntheticRequestId() {
  return `req_syn_${randomUUID().replace(/-/g, "").slice(0, 24)}`;
}
var Session = class {
  sessionId;
  projectPath;
  jsonlPath;
  _records = [];
  _pendingRecords = [];
  _lastUuid = null;
  _slug;
  _cwd;
  _version;
  _gitBranch;
  _model;
  _fileExists;
  _nextTimestamp;
  constructor(opts) {
    this.sessionId = opts.sessionId;
    this.projectPath = opts.projectPath;
    this.jsonlPath = opts.jsonlPath;
    this._slug = opts.slug ?? generateSlug();
    this._cwd = opts.cwd ?? opts.projectPath;
    this._version = opts.version ?? "2.1.83";
    this._gitBranch = opts.gitBranch ?? "HEAD";
    this._model = opts.model ?? "claude-sonnet-4-6";
    this._records = opts.records ?? [];
    this._fileExists = opts.fileExists ?? false;
    this._nextTimestamp = Date.now();
    for (let i = this._records.length - 1; i >= 0; i--) {
      const r = this._records[i];
      if (r.type === "user" || r.type === "assistant" || r.type === "attachment") {
        this._lastUuid = r.uuid;
        break;
      }
    }
  }
  /** All records (existing + pending). */
  get records() {
    return [...this._records, ...this._pendingRecords];
  }
  /** Only user and assistant message records. */
  get messages() {
    return this.records.filter(
      (r) => r.type === "user" || r.type === "assistant"
    );
  }
  /** Only attachment records — Claude Code's injected context (`@file`
   *  expansions, skill listings, task reminders). */
  get attachments() {
    return this.records.filter((r) => r.type === "attachment");
  }
  baseFields() {
    const uuid = randomUUID();
    const record = {
      uuid,
      parentUuid: this._lastUuid,
      sessionId: this.sessionId,
      timestamp: new Date(this._nextTimestamp++).toISOString(),
      isSidechain: false,
      cwd: this._cwd,
      userType: "external",
      version: this._version,
      gitBranch: this._gitBranch,
      slug: this._slug,
      entrypoint: "cli"
    };
    this._lastUuid = uuid;
    return record;
  }
  /** Add a user message, as plain text or content blocks. Returns its uuid. */
  addUserMessage(content) {
    if (Array.isArray(content) && content.length === 0) {
      throw new Error("addUserMessage: content array is empty; Anthropic rejects empty message content");
    }
    const base = this.baseFields();
    const record = {
      type: "user",
      ...base,
      message: { role: "user", content }
    };
    this._pendingRecords.push(record);
    return base.uuid;
  }
  /**
   * Append a record verbatim. The escape hatch for record types this library
   * does not model — `queue-operation`, `last-prompt`, anything a future Claude
   * Code release adds — and for carrying records from one session into another.
   *
   * Dangerous because nothing is synthesized or checked beyond `sessionId` and
   * the parent link: the shape is whatever you pass, and Claude Code will read
   * it back. `sessionId` is overwritten with this session's, since a record
   * claiming another session is never what a caller wants. The record does not
   * join the uuid chain, so a following message still parents to the last real
   * message.
   *
   * The one guard is on `parentUuid`, because that is the failure that does not
   * announce itself: a dangling parent makes Claude Code resume with empty
   * context and answer confidently from nothing.
   *
   * Carrying an *attachment* into another session should use `addAttachment`
   * instead: this method does not advance the chain, so an attachment appended
   * through it becomes a leaf the next message skips over.
   */
  dangerousAppendRecord(record, opts) {
    const parentUuid = opts?.parentUuid !== void 0 ? opts.parentUuid : record.parentUuid;
    if (parentUuid != null) {
      const known = this.records.some((r) => r.uuid === parentUuid);
      if (!known) {
        throw new Error(
          `dangerousAppendRecord: parentUuid ${parentUuid} is not a record in this session; a dangling parent makes Claude Code resume with empty context`
        );
      }
    }
    this._pendingRecords.push({
      ...record,
      ...parentUuid !== void 0 ? { parentUuid } : {},
      sessionId: this.sessionId
    });
  }
  /**
   * Add an attachment record. `parentUuid` defaults to the record this session
   * would currently chain from; pass it explicitly when re-attaching a record
   * carried over from another session, where the message it belongs to has been
   * given a new uuid. Returns the new record's uuid.
   *
   * Attachments are links in the uuid chain, not leaves hanging off it: the
   * record that follows one parents to the attachment, so this advances the
   * chain exactly as the message methods do. Measured across 605 real sessions —
   * 3,526 records chain through an attachment, none skip it.
   */
  addAttachment(attachment, opts) {
    const uuid = randomUUID();
    const record = {
      type: "attachment",
      attachment,
      uuid,
      parentUuid: opts?.parentUuid !== void 0 ? opts.parentUuid : this._lastUuid,
      sessionId: this.sessionId,
      timestamp: new Date(this._nextTimestamp++).toISOString(),
      isSidechain: false,
      cwd: this._cwd,
      userType: "external",
      version: this._version,
      gitBranch: this._gitBranch,
      slug: this._slug,
      entrypoint: "cli"
    };
    this.dangerousAppendRecord(record);
    this._lastUuid = uuid;
    return uuid;
  }
  /** Add an assistant message with the given content blocks. Returns its uuid. */
  addAssistantMessage(content, opts) {
    const base = this.baseFields();
    const hasToolUse = content.some((b) => b.type === "tool_use");
    const payload = {
      id: syntheticMessageId(),
      type: "message",
      role: "assistant",
      model: opts?.model ?? this._model,
      content,
      stop_reason: opts?.stopReason ?? (hasToolUse ? "tool_use" : "end_turn"),
      stop_sequence: null,
      usage: { input_tokens: 0, output_tokens: 0 }
    };
    const record = {
      type: "assistant",
      ...base,
      requestId: syntheticRequestId(),
      message: payload
    };
    this._pendingRecords.push(record);
    return base.uuid;
  }
  /** Add a user message containing tool results. Returns its uuid. */
  addToolResults(results) {
    const base = this.baseFields();
    const record = {
      type: "user",
      ...base,
      message: {
        role: "user",
        content: results.map((r) => ({
          type: "tool_result",
          tool_use_id: r.toolUseId,
          content: r.content,
          ...r.isError ? { is_error: true } : {}
        }))
      }
    };
    this._pendingRecords.push(record);
    return base.uuid;
  }
  /**
   * Convenience: add a complete tool call round-trip.
   * Creates assistant tool_use message, user tool_result message,
   * and optionally a final assistant text response.
   */
  addToolCalls(calls, opts) {
    const toolUseBlocks = calls.map((c) => ({
      type: "tool_use",
      id: `toolu_syn_${randomUUID().replace(/-/g, "").slice(0, 20)}`,
      name: c.name,
      input: c.input
    }));
    this.addAssistantMessage(toolUseBlocks, { model: opts?.model, stopReason: "tool_use" });
    this.addToolResults(
      calls.map((c, i) => ({
        toolUseId: toolUseBlocks[i].id,
        content: c.result,
        isError: c.isError
      }))
    );
    if (opts?.response) {
      this.addAssistantMessage(opts.response, { model: opts?.model });
    }
  }
  /**
   * Import an array of Anthropic API-shaped messages, dispatching each to the
   * appropriate internal method based on role and content type.
   */
  importMessages(messages, opts) {
    const { messages: repaired, origin } = repairWithOrigin(messages);
    const following = /* @__PURE__ */ new Map();
    for (const a of opts?.attachments ?? []) {
      if (a.afterIndex < 0 || a.afterIndex >= messages.length) {
        console.warn(
          `importMessages: attachment afterIndex ${a.afterIndex} is out of range (${messages.length} messages); not emitted`
        );
        continue;
      }
      const list = following.get(a.afterIndex);
      if (list) list.push(a);
      else following.set(a.afterIndex, [a]);
    }
    for (const [i, msg] of repaired.entries()) {
      if (msg.role === "assistant") {
        const content = typeof msg.content === "string" ? [{ type: "text", text: msg.content }] : msg.content;
        this.addAssistantMessage(content);
      } else {
        if (typeof msg.content === "string") {
          this.addUserMessage(msg.content);
        } else {
          const toolResults = msg.content.filter(
            (b) => b.type === "tool_result"
          );
          const rest = msg.content.filter(
            (b) => b.type !== "tool_result"
          );
          if (toolResults.length > 0) {
            this.addToolResults(toolResults.map((r) => ({
              toolUseId: r.tool_use_id,
              content: r.content,
              isError: r.is_error
            })));
          }
          if (rest.length > 0) this.addUserMessage(rest);
        }
      }
      const src = origin[i];
      if (src !== null) {
        for (const a of following.get(src) ?? []) this.addAttachment(a.attachment);
      }
    }
  }
  /**
   * Reset this session to empty state and delete any on-disk artifacts.
   * The sessionId and jsonlPath are preserved so subsequent writes reuse them.
   */
  clear() {
    this._records = [];
    this._pendingRecords = [];
    this._lastUuid = null;
    this._nextTimestamp = Date.now();
    this._fileExists = false;
    removeSessionFiles(this.jsonlPath);
  }
  /** Write pending records to disk. Creates the file/directory if needed. */
  save() {
    if (this._pendingRecords.length === 0) return;
    const dir = dirname(this.jsonlPath);
    if (!existsSync(dir)) {
      mkdirSync(dir, { recursive: true });
    }
    const data = this._pendingRecords.map((r) => serializeRecord(r) + "\n").join("");
    if (this._fileExists) {
      appendFileSync(this.jsonlPath, data, "utf-8");
    } else {
      writeFileSync(this.jsonlPath, data, "utf-8");
      this._fileExists = true;
    }
    this._records.push(...this._pendingRecords);
    this._pendingRecords = [];
  }
};
function removeSessionFiles(jsonlPath) {
  rmSync(jsonlPath, { force: true });
  const companionDir = jsonlPath.endsWith(".jsonl") ? jsonlPath.slice(0, -".jsonl".length) : jsonlPath;
  rmSync(companionDir, { recursive: true, force: true });
}
function deleteSession(sessionId, projectPath, claudeDir) {
  const normalized = normalizeProjectPath(projectPath);
  const jsonlPath = getSessionPath(sessionId, normalized, claudeDir);
  removeSessionFiles(jsonlPath);
}
function createSession(opts) {
  const sessionId = opts.sessionId ?? randomUUID();
  const projectPath = normalizeProjectPath(opts.projectPath);
  const jsonlPath = getSessionPath(sessionId, projectPath, opts.claudeDir);
  return new Session({
    sessionId,
    projectPath,
    jsonlPath,
    cwd: opts.cwd ? normalizeProjectPath(opts.cwd) : projectPath,
    version: opts.version,
    gitBranch: opts.gitBranch,
    model: opts.model
  });
}
function openSession(opts) {
  const projectPath = normalizeProjectPath(opts.projectPath);
  const jsonlPath = getSessionPath(opts.sessionId, projectPath, opts.claudeDir);
  return readSession(jsonlPath, projectPath);
}
function readSession(jsonlPath, projectPath) {
  const records = parseJsonlFile(jsonlPath);
  const firstMsg = records.find((r) => r.type === "user" || r.type === "assistant");
  const sessionId = firstMsg?.sessionId ?? "";
  const resolvedProjectPath = projectPath ?? firstMsg?.cwd ?? "";
  return new Session({
    sessionId,
    projectPath: resolvedProjectPath,
    jsonlPath,
    slug: firstMsg?.slug,
    cwd: firstMsg?.cwd,
    version: firstMsg?.version,
    gitBranch: firstMsg?.gitBranch,
    records,
    fileExists: true
  });
}

export {
  parseJsonl,
  parseJsonlFile,
  serializeRecord,
  serializeJsonl,
  getClaudeDir,
  normalizeProjectPath,
  projectPathToHash,
  getProjectDir,
  getSessionPath,
  repairToolPairing,
  repairWithOrigin,
  Session,
  deleteSession,
  createSession,
  openSession,
  readSession
};
