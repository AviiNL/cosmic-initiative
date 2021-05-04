use std::collections::HashMap;
use std::sync::{Arc, mpsc};

use crate::actor::{Actor, ActorCreate, ActorKey, ActorKind, ActorSelect};
use crate::error::Error;
use crate::frame::ActorMessage;
use crate::label::{Labels, LabelSelectionCriteria};
use crate::org::OrgKey;
use crate::star::{ActorCommand, StarCommand, StarKey};
use crate::user::UserKey;

pub mod system;

pub type AppKind = String;

#[derive(Clone,Hash,Eq,PartialEq)]
pub struct AppKey
{
    pub tenant: TenantKey,
    pub id: u64
}

impl AppKey
{
    pub fn new(tenant: TenantKey, id: u64)->Self
    {
        AppKey{
            tenant: TenantKey,
            id: id
        }
    }
}


#[derive(Clone,Serialize,Deserialize)]
pub struct AppCommandWrapper
{
    app: AppKey,
    user: UserKey,
    payload: AppCommand
}

#[derive(Clone,Serialize,Deserialize)]
pub enum AppCommand
{
    AppMessageExt(AppMessageExt),
    ActorCreate(ActorCreate),
    ActorSelect(ActorSelect),
    ActorDestroy(ActorKey)
}

pub type AppMessageKind = String;

#[derive(Clone,Serialize,Deserialize)]
pub struct AppMessageExt
{
    pub kind: AppMessageKind,
    pub data: Arc<Vec<u8>>
}

#[derive(Clone,Serialize,Deserialize)]
pub struct AppSelect
{
    criteria: Vec<LabelSelectionCriteria>
}

#[derive(Clone,Serialize,Deserialize)]
pub struct AppCreate
{
    pub name: Option<String>,
    pub kind: AppKind,
    pub data: Vec<u8>,
    pub labels: HashMap<String,String>
}

#[derive(Clone,Serialize,Deserialize)]
pub struct AppDestroy
{

}


#[derive(Clone,Serialize,Deserialize)]
pub enum ApplicationStatus
{
    None,
    Launching,
    Ready
}

#[derive(Clone,Serialize,Deserialize)]
pub struct AppInfo
{
    pub key: AppKey,
    pub kind: AppKind
}

impl AppInfo
{
    pub fn new( key: AppKey, kind: AppKind ) -> Self
    {
        AppInfo
        {
            key: key,
            kind: kind
        }
    }
}

#[derive(Clone,Serialize,Deserialize)]
pub struct AppLocation
{
    pub app: AppKey,
    pub supervisor: StarKey
}

#[derive(Clone)]
pub struct AppController
{
    pub app: AppKey,
    pub tx: mpsc::Sender<AppCommandWrapper>
}

pub type Apps = HashMap<AppKind,Box<dyn Application>>;

pub struct AppContext
{
    pub star_tx: mpsc::Sender<AppCommandWrapper>,
    pub info: AppInfo
}


#[async_trait]
pub trait Application
{
    async fn create( &self, context: &AppContext, create: AppCreate ) -> Result<Labels,Error>;
    async fn destroy( &self, context: &AppContext, destroy: AppDestroy ) -> Result<(),Error>;
    async fn handle_app_command(&self, context: &AppContext, command: AppCommandWrapper) -> Result<(),Error>;
    async fn handle_actor_message( &self, context: &AppContext, actor: &mut Actor, message: ActorMessage  ) -> Result<(),Error>;
}
