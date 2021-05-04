use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;

use crate::app::AppKey;
use crate::error::Error;
use crate::frame::{Event, ActorMessage, ActorState};
use crate::id::Id;
use crate::label::LabelSelectionCriteria;
use crate::star::StarKey;

pub static DEFAULT_ENTITY_KIND_EXT: &str = "default";
pub static DEFAULT_GATHERING_KIND_EXT: &str = "default";

#[derive(Eq,PartialEq,Hash,Clone,Serialize,Deserialize)]
pub struct ActorInfo
{
    pub key: ActorKey,
    pub kind: ActorKind
}

#[derive(Eq,PartialEq,Hash,Clone,Serialize,Deserialize)]
pub struct ActorKey
{
    pub app: AppKey,
    pub id: Id,
}


pub trait Actor
{
    async fn handle_message(&mut self, message: ActorMessage );
}

pub type ActorKindExt = String;
pub type GatheringKindExt = String;

#[derive(Eq,PartialEq,Hash,Clone,Serialize,Deserialize)]
pub enum ActorKind
{
    Actor(ActorKindExt),
    Gathering(GatheringKindExt)
}

impl ActorKind
{
    pub fn default_entity()->Self {
        ActorKind::Actor(DEFAULT_ENTITY_KIND_EXT.to_string())
    }

    pub fn default_gathering()-> Self {
        ActorKind::Actor(DEFAULT_GATHERING_KIND_EXT.to_string())
    }
}

impl fmt::Display for ActorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{})", self.app, self.id)
    }
}

#[derive(Clone,Serialize,Deserialize)]
pub struct ActorLocation
{
    pub actor: ActorKey,
    pub star: StarKey,
    pub gathering: Option<ActorKey>,
    pub ext: Option<Vec<u8>>
}

impl ActorLocation
{
    pub fn new(resource: ActorKey, star: StarKey ) ->Self
    {
        ActorLocation {
            actor: resource,
            star: star,
            ext: Option::None,
            gathering: Option::None
        }
    }

    pub fn new_ext(resource: ActorKey, star: StarKey, ext: Vec<u8>) -> Self
    {
        ActorLocation {
            actor: resource,
            star: star,
            ext: Option::Some(ext),
            gathering: Option::None
        }
    }
}

pub struct ActorGathering
{
    pub key: ActorKey,
    pub entity: Vec<ActorKey>
}


pub struct ActorWatcher
{
    pub entity: ActorKey,
    pub tx: Sender<Event>
}

impl ActorWatcher
{
    pub fn new(entity: ActorKey) ->(Self, broadcast::Receiver<Event>)
    {
        let (tx,rx) = broadcast::channel(32);
        (ActorWatcher {
            entity,
            tx: tx
        }, rx)
    }
}

impl ActorWatcher
{
    pub fn notify( &self, event: Event)
    {
        self.tx.send(event);
    }
}

#[derive(Clone,Serialize,Deserialize)]
pub struct ActorCreate
{
    pub app: AppKey,
    pub kind: ActorKind,
    pub data: Arc<Vec<u8>>,
    pub labels: HashMap<String,String>
}

#[derive(Clone,Serialize,Deserialize)]
pub struct ActorSelect
{
    criteria: Vec<LabelSelectionCriteria>
}

