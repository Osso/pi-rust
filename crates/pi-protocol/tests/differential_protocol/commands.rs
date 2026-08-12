use pi_protocol::{
    AbortCommand, AttachCommand, Command, CreateCommand, CreateCommandInput, DetachCommand, ListCommand, ModelRef,
    PromptCommand, SetModelCommand, SetThinkingCommand, SteerCommand,
};
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 8000;
const FIXTURE_COUNT: u64 = 22;

#[derive(Clone, Copy)]
struct RawModel {
    provider: &'static str,
    id: &'static str,
}

struct Fixture {
    name: &'static str,
    command: &'static str,
    cwd: Option<&'static str>,
    name_value: Option<&'static str>,
    model: Option<RawModel>,
    thinking_level: Option<&'static str>,
    session_id: Option<&'static str>,
    text: Option<&'static str>,
}

impl Fixture {
    const fn new(name: &'static str, command: &'static str) -> Self {
        Self {
            name,
            command,
            cwd: None,
            name_value: None,
            model: None,
            thinking_level: None,
            session_id: None,
            text: None,
        }
    }

    fn request(&self, id: u64) -> Value {
        let mut value = json!({"command": self.command});
        insert_optional_string(&mut value, "cwd", self.cwd);
        insert_optional_string(&mut value, "name", self.name_value);
        insert_optional_model(&mut value, self.model);
        insert_optional_string(&mut value, "thinkingLevel", self.thinking_level);
        insert_optional_string(&mut value, "sessionId", self.session_id);
        insert_optional_string(&mut value, "text", self.text);
        json!({
            "id": id,
            "operation": "validate_command",
            "value": value,
        })
    }

    fn rust_response(&self, id: u64) -> Value {
        let command = match self.construct_command() {
            Ok(command) => command,
            Err(()) => return invalid_response(id),
        };
        valid_response(id, normalize_command(&command))
    }

    fn construct_command(&self) -> Result<Command, ()> {
        match self.command {
            "list" => Ok(Command::List(ListCommand::new())),
            "create" => self.construct_create(),
            "attach" => self.construct_attach(),
            "detach" => self.construct_detach(),
            "prompt" => self.construct_prompt(),
            "steer" => self.construct_steer(),
            "abort" => self.construct_abort(),
            "set_model" => self.construct_set_model(),
            "set_thinking" => self.construct_set_thinking(),
            _ => Err(()),
        }
    }

    fn construct_create(&self) -> Result<Command, ()> {
        let model = self.model.map(parse_model).transpose()?;
        let thinking_level = self.thinking_level.map(str::parse).transpose().map_err(|_| ())?;
        let command = CreateCommand::new(CreateCommandInput {
            cwd: self.cwd.map(str::to_owned),
            name: self.name_value.map(str::to_owned),
            model,
            thinking_level,
        })
        .map_err(|_| ())?;
        Ok(Command::Create(command))
    }

    fn construct_attach(&self) -> Result<Command, ()> {
        let command = AttachCommand::new(required(self.session_id)?).map_err(|_| ())?;
        Ok(Command::Attach(command))
    }

    fn construct_detach(&self) -> Result<Command, ()> {
        let command = DetachCommand::new(required(self.session_id)?).map_err(|_| ())?;
        Ok(Command::Detach(command))
    }

    fn construct_prompt(&self) -> Result<Command, ()> {
        let command = PromptCommand::new(required(self.session_id)?, required(self.text)?).map_err(|_| ())?;
        Ok(Command::Prompt(command))
    }

    fn construct_steer(&self) -> Result<Command, ()> {
        let command = SteerCommand::new(required(self.session_id)?, required(self.text)?).map_err(|_| ())?;
        Ok(Command::Steer(command))
    }

    fn construct_abort(&self) -> Result<Command, ()> {
        let command = AbortCommand::new(required(self.session_id)?).map_err(|_| ())?;
        Ok(Command::Abort(command))
    }

    fn construct_set_model(&self) -> Result<Command, ()> {
        let model = self.model.map(parse_model).transpose()?;
        let command = SetModelCommand::new(required(self.session_id)?, model.ok_or(())?).map_err(|_| ())?;
        Ok(Command::SetModel(command))
    }

    fn construct_set_thinking(&self) -> Result<Command, ()> {
        let thinking_level = self.thinking_level.ok_or(())?.parse().map_err(|_| ())?;
        let command = SetThinkingCommand::new(required(self.session_id)?, thinking_level).map_err(|_| ())?;
        Ok(Command::SetThinking(command))
    }
}

fn parse_model(model: RawModel) -> Result<ModelRef, ()> {
    ModelRef::new(model.provider.to_owned(), model.id.to_owned()).map_err(|_| ())
}

fn required(value: Option<&str>) -> Result<String, ()> {
    value.map(str::to_owned).ok_or(())
}

fn insert_optional_string(value: &mut Value, key: &str, text: Option<&str>) {
    if let Some(text) = text {
        value[key] = json!(text);
    }
}

fn insert_optional_model(value: &mut Value, model: Option<RawModel>) {
    if let Some(model) = model {
        value["model"] = json!({"provider": model.provider, "id": model.id});
    }
}

fn normalize_command(command: &Command) -> Value {
    let command_name = command.as_str();
    match command {
        Command::List(_) => json!({"command": command_name}),
        Command::Create(create) => normalize_create(create, command_name),
        Command::Attach(attach) => json!({"command": command_name, "sessionId": attach.session_id()}),
        Command::Detach(detach) => json!({"command": command_name, "sessionId": detach.session_id()}),
        Command::Prompt(prompt) => json!({
            "command": command_name,
            "sessionId": prompt.session_id(),
            "text": prompt.text(),
        }),
        Command::Steer(steer) => json!({
            "command": command_name,
            "sessionId": steer.session_id(),
            "text": steer.text(),
        }),
        Command::Abort(abort) => json!({"command": command_name, "sessionId": abort.session_id()}),
        Command::SetModel(set_model) => json!({
            "command": command_name,
            "sessionId": set_model.session_id(),
            "model": normalize_model(set_model.model()),
        }),
        Command::SetThinking(set_thinking) => json!({
            "command": command_name,
            "sessionId": set_thinking.session_id(),
            "thinkingLevel": set_thinking.thinking_level().as_str(),
        }),
    }
}

fn normalize_create(command: &CreateCommand, command_name: &str) -> Value {
    let mut value = json!({"command": command_name});
    insert_optional_string(&mut value, "cwd", command.cwd());
    insert_optional_string(&mut value, "name", command.name());
    if let Some(model) = command.model() {
        value["model"] = normalize_model(model);
    }
    if let Some(thinking_level) = command.thinking_level() {
        value["thinkingLevel"] = json!(thinking_level.as_str());
    }
    value
}

fn normalize_model(model: &ModelRef) -> Value {
    json!({"provider": model.provider(), "id": model.id()})
}

fn valid_response(id: u64, value: Value) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": true, "value": value},
    })
}

fn invalid_response(id: u64) -> Value {
    json!({
        "id": id,
        "ok": true,
        "output": {"valid": false},
    })
}

fn configured_fixture(name: &'static str, command: &'static str, configure: impl FnOnce(&mut Fixture)) -> Fixture {
    let mut fixture = Fixture::new(name, command);
    configure(&mut fixture);
    fixture
}

const fn raw_model(provider: &'static str, id: &'static str) -> RawModel {
    RawModel { provider, id }
}

fn valid_fixtures() -> Vec<Fixture> {
    let mut fixtures = vec![Fixture::new("list", "list"), Fixture::new("create omitted", "create")];
    fixtures.extend(create_fixtures());
    fixtures.extend(session_fixtures());
    fixtures
}

fn create_fixtures() -> Vec<Fixture> {
    vec![
        configured_fixture("create cwd", "create", |fixture| fixture.cwd = Some("/workspace")),
        configured_fixture("create empty name", "create", |fixture| fixture.name_value = Some("")),
        configured_fixture("create model", "create", |fixture| {
            fixture.model = Some(raw_model("openai", "gpt-5"));
        }),
        configured_fixture("create thinking", "create", |fixture| {
            fixture.thinking_level = Some("high");
        }),
        configured_fixture("create all options", "create", |fixture| {
            fixture.cwd = Some(" \tworkspace\0");
            fixture.name_value = Some("模型\0");
            fixture.model = Some(raw_model(" provider ", "モデル"));
            fixture.thinking_level = Some("xhigh");
        }),
    ]
}

fn session_fixtures() -> Vec<Fixture> {
    vec![
        configured_fixture("attach", "attach", |fixture| fixture.session_id = Some("session\0é")),
        configured_fixture("detach", "detach", |fixture| fixture.session_id = Some("session-2")),
        configured_fixture("prompt empty text", "prompt", |fixture| {
            fixture.session_id = Some("session-3");
            fixture.text = Some("");
        }),
        configured_fixture("steer Unicode NUL text", "steer", |fixture| {
            fixture.session_id = Some("session-4");
            fixture.text = Some("é\0𐅑");
        }),
        configured_fixture("abort", "abort", |fixture| fixture.session_id = Some("session-5")),
        configured_fixture("set model", "set_model", |fixture| {
            fixture.session_id = Some("session-6");
            fixture.model = Some(raw_model("anthropic", "claude"));
        }),
        configured_fixture("set thinking", "set_thinking", |fixture| {
            fixture.session_id = Some("session-7");
            fixture.thinking_level = Some("minimal");
        }),
    ]
}

fn invalid_fixtures() -> Vec<Fixture> {
    vec![
        configured_fixture("empty create cwd", "create", |fixture| fixture.cwd = Some("")),
        configured_fixture("empty attach session id", "attach", |fixture| {
            fixture.session_id = Some("")
        }),
        configured_fixture("empty prompt session id", "prompt", |fixture| {
            fixture.session_id = Some("");
            fixture.text = Some("");
        }),
        configured_fixture("empty set model session id", "set_model", |fixture| {
            fixture.session_id = Some("");
            fixture.model = Some(raw_model("openai", "gpt-5"));
        }),
        configured_fixture("empty set model provider", "set_model", |fixture| {
            fixture.session_id = Some("session");
            fixture.model = Some(raw_model("", "gpt-5"));
        }),
        configured_fixture("empty set model id", "set_model", |fixture| {
            fixture.session_id = Some("session");
            fixture.model = Some(raw_model("openai", ""));
        }),
        configured_fixture("empty set thinking session id", "set_thinking", |fixture| {
            fixture.session_id = Some("");
            fixture.thinking_level = Some("high");
        }),
        configured_fixture("ultra set thinking", "set_thinking", |fixture| {
            fixture.session_id = Some("session");
            fixture.thinking_level = Some("ultra");
        }),
    ]
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = valid_fixtures().into_iter().chain(invalid_fixtures());
    let mut next_id = FIRST_REQUEST_ID;
    for fixture in fixtures {
        let expected = oracle.request(&fixture.request(next_id));
        let actual = fixture.rust_response(next_id);
        assert_eq!(actual, expected, "command mismatch: {}", fixture.name);
        next_id += 1;
    }
    assert_eq!(next_id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}
