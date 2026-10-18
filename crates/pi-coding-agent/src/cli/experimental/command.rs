//! Minimal typed command parser (`cli/experimental/command.ts`).

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

/// An invocation identifies the command that produced it.
pub trait NamedCommandInvocation {
    fn command(&self) -> &str;
}

pub enum CommandParseResult<T> {
    Ok(T),
    Errors(Vec<String>),
}

impl<T> CommandParseResult<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}

impl<T: PartialEq> PartialEq for CommandParseResult<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ok(left), Self::Ok(right)) => left == right,
            (Self::Errors(left), Self::Errors(right)) => left == right,
            _ => false,
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for CommandParseResult<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok(value) => formatter.debug_tuple("Ok").field(value).finish(),
            Self::Errors(errors) => formatter.debug_tuple("Errors").field(errors).finish(),
        }
    }
}

type OptionParser<T> = Arc<dyn Fn(&str) -> Result<T, String> + Send + Sync>;

pub struct CommandOption<T> {
    name: String,
    parse: OptionParser<T>,
}

impl<T> Clone for CommandOption<T> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            parse: Arc::clone(&self.parse),
        }
    }
}

impl<T> CommandOption<T> {
    pub fn value(name: impl Into<String>, parse: impl Fn(&str) -> Result<T, String> + Send + Sync + 'static) -> Self {
        Self {
            name: name.into(),
            parse: Arc::new(parse),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn value_option<T>(
    name: impl Into<String>,
    parse: impl Fn(&str) -> Result<T, String> + Send + Sync + 'static,
) -> CommandOption<T> {
    CommandOption::value(name, parse)
}

pub fn string_option(name: impl Into<String>) -> CommandOption<String> {
    CommandOption::value(name, |value| Ok(value.to_owned()))
}

trait ErasedOption: Send + Sync {
    fn name(&self) -> &str;
    fn parse(&self, value: &str) -> Result<Box<dyn Any + Send + Sync>, String>;
}

impl<T: Any + Send + Sync> ErasedOption for CommandOption<T> {
    fn name(&self) -> &str {
        &self.name
    }

    fn parse(&self, value: &str) -> Result<Box<dyn Any + Send + Sync>, String> {
        (self.parse)(value).map(|parsed| Box::new(parsed) as Box<dyn Any + Send + Sync>)
    }
}

type ParsedValues = BTreeMap<String, Vec<Box<dyn Any + Send + Sync>>>;

pub struct ParsedCommandInput<'a> {
    pub remaining_args: &'a [String],
    pub errors: &'a [String],
    values: &'a ParsedValues,
}

impl ParsedCommandInput<'_> {
    pub fn value<T: Clone + 'static>(&self, option: &CommandOption<T>) -> Option<T> {
        self.values.get(option.name())?.first()?.downcast_ref::<T>().cloned()
    }

    pub fn values<T: Clone + 'static>(&self, option: &CommandOption<T>) -> Vec<T> {
        self.values
            .get(option.name())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.downcast_ref::<T>().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

type Builder<TOwn> = Box<dyn for<'a> Fn(&ParsedCommandInput<'a>) -> CommandParseResult<TOwn> + Send + Sync>;
type Action<TOwn, TCtx> = Box<dyn Fn(&TOwn, &mut TCtx) + Send + Sync>;
type ParseFn<TOwn> = Box<dyn Fn(&[String]) -> CommandParseResult<TOwn> + Send + Sync>;
type ExecuteFn<TOwn, TCtx> = Box<dyn Fn(&[String], &mut TCtx) -> Result<TOwn, Vec<String>> + Send + Sync>;

struct Subcommand<TOwn, TCtx> {
    name: String,
    parse: ParseFn<TOwn>,
    execute: ExecuteFn<TOwn, TCtx>,
}

pub struct Command<TOwn, TCtx> {
    name: String,
    options: Vec<Box<dyn ErasedOption>>,
    subcommands: Vec<Subcommand<TOwn, TCtx>>,
    builder: Option<Builder<TOwn>>,
    action: Option<Action<TOwn, TCtx>>,
}

impl<TOwn: NamedCommandInvocation + 'static, TCtx: 'static> Command<TOwn, TCtx> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            options: Vec::new(),
            subcommands: Vec::new(),
            builder: None,
            action: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Register an option; a duplicate name is a programmer error, as in the source.
    pub fn option<T: Any + Send + Sync>(&mut self, option: CommandOption<T>) -> &mut Self {
        if self.options.iter().any(|existing| existing.name() == option.name()) {
            panic!("Option {} is already registered for {}", option.name(), self.name);
        }
        self.options.push(Box::new(option));
        self
    }

    pub fn build(
        &mut self,
        builder: impl for<'a> Fn(&ParsedCommandInput<'a>) -> CommandParseResult<TOwn> + Send + Sync + 'static,
    ) -> &mut Self {
        self.builder = Some(Box::new(builder));
        self
    }

    pub fn action(&mut self, action: impl Fn(&TOwn, &mut TCtx) + Send + Sync + 'static) -> &mut Self {
        self.action = Some(Box::new(action));
        self
    }

    /// Register a subcommand, converting its invocation into this command's invocation type.
    pub fn command<TSub: NamedCommandInvocation + Send + Sync + 'static>(
        &mut self,
        subcommand: Command<TSub, TCtx>,
        convert: impl Fn(TSub) -> TOwn + Send + Sync + 'static,
    ) -> &mut Self {
        if self.subcommands.iter().any(|existing| existing.name == subcommand.name) {
            panic!("Command {} is already registered", subcommand.name);
        }
        let name = subcommand.name.clone();
        let subcommand = Arc::new(subcommand);
        let convert = Arc::new(convert);
        let parse_subcommand = Arc::clone(&subcommand);
        let convert_parse = Arc::clone(&convert);
        let execute_subcommand = Arc::clone(&subcommand);
        let convert_execute = Arc::clone(&convert);
        self.subcommands.push(Subcommand {
            name,
            parse: Box::new(move |argv| match parse_subcommand.parse(argv) {
                CommandParseResult::Ok(invocation) => CommandParseResult::Ok(convert_parse(invocation)),
                CommandParseResult::Errors(errors) => CommandParseResult::Errors(errors),
            }),
            execute: Box::new(move |argv, context| {
                execute_subcommand
                    .execute(argv, context)
                    .map(|invocation| convert_execute(invocation))
            }),
        });
        self
    }

    pub fn parse(&self, argv: &[String]) -> CommandParseResult<TOwn> {
        if let Some(subcommand) = self.select(argv) {
            return (subcommand.parse)(&argv[1..]);
        }
        self.parse_own(argv)
    }

    pub fn execute(&self, argv: &[String], context: &mut TCtx) -> Result<TOwn, Vec<String>> {
        if let Some(subcommand) = self.select(argv) {
            return (subcommand.execute)(&argv[1..], context);
        }
        let command = match self.parse_own(argv) {
            CommandParseResult::Ok(command) => command,
            CommandParseResult::Errors(errors) => return Err(errors),
        };
        let action = self
            .action
            .as_ref()
            .unwrap_or_else(|| panic!("Command {} does not define an action", self.name));
        action(&command, context);
        Ok(command)
    }

    fn select(&self, argv: &[String]) -> Option<&Subcommand<TOwn, TCtx>> {
        let candidate = argv.first()?;
        self.subcommands.iter().find(|subcommand| &subcommand.name == candidate)
    }

    fn parse_own(&self, argv: &[String]) -> CommandParseResult<TOwn> {
        let builder = self
            .builder
            .as_ref()
            .unwrap_or_else(|| panic!("Command {} does not define a builder", self.name));
        let ParsedOptions {
            values,
            remaining_args,
            errors,
        } = self.parse_options(argv);
        let input = ParsedCommandInput {
            remaining_args: &remaining_args,
            errors: &errors,
            values: &values,
        };
        match builder(&input) {
            CommandParseResult::Ok(command) if errors.is_empty() => CommandParseResult::Ok(command),
            CommandParseResult::Ok(_) => CommandParseResult::Errors(errors),
            CommandParseResult::Errors(mut builder_errors) => {
                let mut all = errors;
                all.append(&mut builder_errors);
                if all.is_empty() {
                    panic!("Command {} failed without an error", self.name);
                }
                CommandParseResult::Errors(all)
            }
        }
    }

    fn parse_options(&self, argv: &[String]) -> ParsedOptions {
        let mut parsed = ParsedOptions::default();
        let mut index = 0;
        while index < argv.len() {
            let argument = &argv[index];
            if argument == "--" {
                parsed.remaining_args.extend(argv[index..].iter().cloned());
                break;
            }
            let equals = argument.find('=');
            let name = equals.map_or(argument.as_str(), |position| &argument[..position]);
            let Some(option) = self.options.iter().find(|option| option.name() == name) else {
                parsed.remaining_args.extend(argv[index..].iter().cloned());
                break;
            };
            let mut value = equals.map(|position| argument[position + 1..].to_owned());
            if value.is_none() {
                let next = argv.get(index + 1);
                if next.is_some_and(|next| !next.starts_with('-')) {
                    value = next.cloned();
                    index += 1;
                }
            }
            let Some(value) = value.filter(|value| !value.is_empty()) else {
                parsed.errors.push(format!("{name} requires a value"));
                index += 1;
                continue;
            };
            if parsed.values.contains_key(name) {
                parsed.errors.push(format!("{name} may only be specified once"));
                index += 1;
                continue;
            }
            match option.parse(&value) {
                Ok(parsed_value) => {
                    parsed.values.insert(name.to_owned(), vec![parsed_value]);
                }
                Err(error) => parsed.errors.push(error),
            }
            index += 1;
        }
        parsed
    }
}

#[derive(Default)]
struct ParsedOptions {
    values: ParsedValues,
    remaining_args: Vec<String>,
    errors: Vec<String>,
}
