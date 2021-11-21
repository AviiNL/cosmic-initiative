use crate::error::Error;
use crate::frame::StarPattern;
use crate::mesh::serde::id::{AddressAndKind, ResourceType};
use crate::mesh::serde::payload::Payload;
use crate::mesh::serde::payload::Primitive;
use crate::resource::{DomainCase, ResourceAddressPartKind};
use crate::star::StarKind;
use actix_web::web::block;
use mesh_portal_parse::parse::skewer;
use nom::branch::alt;
use nom::bytes::complete::{tag, take};
use nom::character::complete::{alpha1, anychar, multispace0, multispace1};
use nom::combinator::{all_consuming, not, opt};
use nom::error::{context, ErrorKind};
use nom::multi::{many0, many1, separated_list0};
use nom::sequence::{delimited, preceded, terminated, tuple};
use nom::{AsChar, InputTakeAtPosition};
use nom_supreme::parse_from_str;
use starlane_resources::message::MessageFrom;
use starlane_resources::parse::{parse_domain, parse_resource_path, parse_resource_path_and_kind};
use starlane_resources::{
    AddressCreationSrc, AssignResourceStateSrc, ConfigSrc, KeyCreationSrc, Res, ResourceAddress,
    ResourceArchetype, ResourceCreate, ResourceCreateStrategy, ResourceIdentifier, ResourcePath,
    ResourcePathAndKind, ResourceSelector,
};
use std::str::FromStr;
use std::collections::HashMap;

pub fn parse_star_kind(input: &str) -> Res<&str, Result<StarKind, Error>> {
    context("star_kind", delimited(tag("<"), alpha1, tag(">")))(input).map(|(input_next, kind)| {
        match StarKind::from_str(kind) {
            Ok(kind) => (input_next, Ok(kind)),
            Err(error) => (input_next, Err(error.into())),
        }
    })
}

pub fn parse_star_pattern(input: &str) -> Res<&str, Result<StarPattern, Error>> {
    context("star_pattern", parse_star_kind)(input).map(|(input_next, kind)| match kind {
        Ok(kind) => (input_next, Ok(StarPattern::StarKind(kind))),
        Err(error) => (input_next, Err(error.into())),
    })
}

fn alpha1_hyphen<T>(i: T) -> Res<T, T>
where
    T: InputTakeAtPosition,
    <T as InputTakeAtPosition>::Item: AsChar,
{
    i.split_at_position1_complete(
        |item| {
            let char_item = item.as_char();
            !(char_item == '-') && !(char_item.is_alpha() || char_item.is_dec_digit())
        },
        ErrorKind::AlphaNumeric,
    )
}

fn not_quote<T>(i: T) -> Res<T, T>
where
    T: InputTakeAtPosition,
    <T as InputTakeAtPosition>::Item: AsChar,
{
    i.split_at_position1_complete(
        |item| {
            let char_item = item.as_char();
            (char_item == '"')
        },
        ErrorKind::AlphaNumeric,
    )
}

pub fn parse_host(input: &str) -> Res<&str, &str> {
    context("parse_host", terminated(alpha1_hyphen, tag(":")))(input)
}

pub fn text_value(input: &str) -> Res<&str, Payload> {
    delimited(tag("\""), not_quote, tag("\""))(input)
        .map(|(next, text)| (next, Payload::Single(Primitive::Text(text.to_string()))))
}

pub fn address_value(input: &str) -> Res<&str, Payload> {
    parse_resource_path(input).map(|(next, address)| (next, Payload::Single(Primitive::Address(address.into()))))
}

pub fn value(input: &str) -> Res<&str, Payload> {
    alt((text_value, address_value))(input)
}

pub fn set_directive(input: &str) -> Res<&str, SetDir> {
    tuple((preceded(tag("+"), skewer), tag("="), value))(input).map(|(next, (key, _, value))| {
        (
            next,
            SetDir {
                key: key.to_string(),
                value,
            },
        )
    })
}

pub fn text_payload_block(input: &str) -> Res<&str, Block> {
    delimited(
        tag("+["),
        tuple((
            multispace0,
            delimited(tag("\""), not_quote, tag("\"")),
            multispace0,
        )),
        tag("]"),
    )(input)
    .map(|(next, (_, text, _))| {
        (
            next,
            Block::Payload(Payload::Single(Primitive::Text(text.to_string()))),
        )
    })
}

pub fn upload_pattern_block(input: &str) -> Res<&str, Block> {
    delimited(
        tag("^["),
        tuple((multispace0, filename, multispace0)),
        tag("]"),
    )(input)
    .map(|(next, (_, block, filename))| (next, Block::Upload(filename.to_string())))
}

pub fn pipeline_block(input: &str) -> Res<&str, Block> {
    alt((text_payload_block, upload_pattern_block))(input)
}

pub fn single_block_pipeline_step(input: &str) -> Res<&str, Block> {
    tuple((pipeline_block, (tag("->"))))(input).map(|(next, (block, kind))| (next, block))
}

pub fn create(input: &str) -> Res<&str, Command> {
    tuple((
        delimited(multispace0, tag("create"), multispace0),
        opt(single_block_pipeline_step),
        parse_from_str,
        opt(delimited(
            tag("{"),
            delimited(
                tag(multispace0),
                separated_list0(multispace1, set_directive),
                multispace0,
            ),
            tag("}"),
        )),
    ))(input)
    .map(|(next, (_, block, address_and_kind, sets))| {
        let address_and_kind: ResourcePathAndKind = address_and_kind;
        let sets = match sets {
            None => {
                vec![]
            }
            Some(some) => some,
        };

        let state_src = match block {
            None => StateSrc::None,
            Some(some) => match some {
                Block::Payload(payload) => StateSrc::Direct(payload),
                Block::Upload(_) => StateSrc::FromCommandPayload,
            },
        };

        let mut set_map = HashMap::new();
        for set_dir in sets {
            set_map.insert(set_dir.key,set_dir.value );
        }
        let create = CreateCommand {
            address_and_kind,
            state_src,
            set_directives: set_map,
        };

        (next, Command::Create(create))
    })
}

pub fn select(input: &str) -> Res<&str, Command> {
    tuple((
        delimited(multispace0, tag("select"), multispace0),
        parse_resource_path,
    ))(input)
    .map(|(next, (_, address))| {
        let mut selector =
            ResourceSelector::children_selector(ResourceIdentifier::Address(address));
        (next, Command::Select(selector))
    })
}

pub fn unique(input: &str) -> Res<&str, Command> {
    tuple((
        delimited(multispace0, tag("unique"), multispace0),
        parse_from_str,
    ))(input)
    .map(|(next, (_, resource_type))| {
        let resource_type: ResourceType = resource_type;
        (next, Command::Unique(resource_type))
    })
}

pub fn command(input: &str) -> Res<&str, Command> {
    delimited(multispace0, alt((create, select, unique)), multispace0)(input)
}

pub fn consume_command(input: &str) -> Result<Command, Error> {
    Ok(all_consuming(command)(input)?.1)
}

pub enum Command {
    Create(CreateCommand),
    Select(ResourceSelector),
    Unique(ResourceType),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCommand {
    pub address_and_kind: ResourcePathAndKind,
    pub state_src: StateSrc,
    pub set_directives: HashMap<String,Payload>,
}

impl CreateCommand {
    pub fn parent(&self) -> ResourceIdentifier {
        match self.address_and_kind.path.parent() {
            None => {
                ResourceKey::root().into()
            }
            Some(parent) => {
                parent.into()
            }
        }
    }
}

pub struct SetDir {
    pub key: String,
    pub value: Payload,
}

pub enum StateSrc {
    None,
    Address(ResourcePath),
    Direct(Payload),
    FromCommandPayload,
}



pub enum StatePipeline {}

pub enum Block {
    Payload(Payload),
    Upload(String),
}

pub struct PipelineStep {
    pub blocks: Vec<Block>,
}



