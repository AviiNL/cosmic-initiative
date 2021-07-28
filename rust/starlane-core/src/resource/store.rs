
use std::str::FromStr;
use std::sync::Arc;

use rusqlite::{Connection, params, Row};
use rusqlite::types::ValueRef;
use tokio::sync::{mpsc, oneshot};

use starlane_resources::{ResourceIdentifier, ResourceStatePersistenceManager};

use crate::app::ConfigSrc;
use crate::error::Error;

use crate::message::Fail;
use crate::resource::{DataTransfer, FileDataTransfer, LocalStateSetSrc, MemoryDataTransfer, Resource, ResourceAddress, ResourceArchetype, ResourceAssign, ResourceCreate, ResourceKey, ResourceKind, Specific};
use crate::data::{DataSetBlob, DataSetSrc, LocalBinSrc};
use std::convert::TryInto;

#[derive(Clone,Debug)]
pub struct ResourceStore {
    tx: mpsc::Sender<ResourceStoreAction>,
}

impl ResourceStore {
    pub async fn new() -> Self {
        ResourceStore {
            tx: ResourceStoreSqlLite::new().await,
        }
    }

    pub async fn put(
        &self,
        assign: ResourceAssign<DataSetSrc<LocalBinSrc>>,
    ) -> Result<Resource, Fail> {
        let (tx, rx) = oneshot::channel();

        self.tx
            .send(ResourceStoreAction {
                command: ResourceStoreCommand::Put(assign),
                tx: tx,
            })
            .await?;

        match rx.await?? {
            ResourceStoreResult::Resource(resource) => {
                resource.ok_or(Fail::Error("option returned None".into()))
            }
            _ => Err(Fail::Error(
                "unexpected response from host registry sql".into(),
            )),
        }
    }

    pub async fn get(&self, identifier: ResourceIdentifier) -> Result<Option<Resource>, Fail> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ResourceStoreAction {
                command: ResourceStoreCommand::Get(identifier.clone()),
                tx: tx,
            })
            .await?;
        let result = rx.await??;
        match result {
            ResourceStoreResult::Resource(resource) => Ok(resource),
            what => Err(Fail::Unexpected{ expected: "Resource()".to_string(), received: what.to_string()}),
        }
    }

    pub fn close(&self) {
        let tx = self.tx.clone();
        tokio::spawn( async move {
            tx
                .send(ResourceStoreAction {
                    command: ResourceStoreCommand::Close,
                    tx: oneshot::channel().0
                })
                .await;
        });

    }
}

pub struct ResourceStoreAction {
    pub command: ResourceStoreCommand,
    pub tx: oneshot::Sender<Result<ResourceStoreResult, Fail>>,
}

#[derive(strum_macros::Display)]
pub enum ResourceStoreCommand {
    Close,
    Put(ResourceAssign<DataSetSrc<LocalBinSrc>>),
    Get(ResourceIdentifier),
}

pub enum ResourceStoreResult {
    Ok,
    Resource(Option<Resource>),
}

impl ToString for ResourceStoreResult{
    fn to_string(&self) -> String {
        match self {
            ResourceStoreResult::Ok => "ResourceStoreResult::Ok".to_string(),
            ResourceStoreResult::Resource(_) => "ResourceStoreResult::Resource(_)".to_string()
        }
    }
}

pub struct ResourceStoreSqlLite {
    pub conn: Connection,
    pub tx: mpsc::Sender<ResourceStoreAction>,
    pub rx: mpsc::Receiver<ResourceStoreAction>,
}

impl ResourceStoreSqlLite {
    pub async fn new() -> mpsc::Sender<ResourceStoreAction> {
        let (tx, rx) = mpsc::channel(1024);

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let conn = Connection::open_in_memory();
            if conn.is_ok() {
                let mut db = ResourceStoreSqlLite {
                    conn: conn.unwrap(),
                    tx: tx_clone,
                    rx: rx,
                };
                match db.run().await {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("experienced fatal error in sql db: {}", err);
                    }
                }
            }
        });
        tx
    }

    async fn run(&mut self) -> Result<(), Error> {
        match self.setup() {
            Ok(_) => {}
            Err(err) => {
                eprintln!("error setting up db: {}", err);
                return Err(err);
            }
        };

        while let Option::Some(request) = self.rx.recv().await {
            if let ResourceStoreCommand::Close = request.command {
                request.tx.send(Ok(ResourceStoreResult::Ok));
                break;
            } else {
                request.tx.send(self.process(request.command).await);
            }
        }

        Ok(())
    }

    async fn process(
        &mut self,
        command: ResourceStoreCommand,
    ) -> Result<ResourceStoreResult, Fail> {
        match command {
            ResourceStoreCommand::Close => Ok(ResourceStoreResult::Ok),
            ResourceStoreCommand::Put(assign) => {
                let key = assign.stub.key.bin()?;
                let address = assign.stub.address.to_string();
                let specific = match &assign.stub.archetype.specific {
                    None => Option::None,
                    Some(specific) => Option::Some(specific.to_string()),
                };
                let config_src = match &assign.stub.archetype.config {
                    None => Option::None,
                    Some(config_src) => Option::Some(config_src.to_string()),
                };

                let state = match assign
                    .stub
                    .archetype
                    .kind
                    .resource_type()
                    .state_persistence()
                {
                    ResourceStatePersistenceManager::Store => {
                        let state_src: DataSetBlob = assign.state_src.clone().try_into()?;
                        state_src.bin()?
                    }
                    _ => {
                        DataSetBlob::new().bin()?
                    }
                };

                self.conn.execute("INSERT INTO resources (key,address,state_src,kind,specific,config_src) VALUES (?1,?2,?3,?4,?5,?6)", params![key,address,state,assign.stub.archetype.kind.to_string(),specific,config_src])?;

                let resource = Resource::new(
                    assign.stub.key,
                    assign.stub.address,
                    assign.stub.archetype,
                    assign.state_src
                );

                Ok(ResourceStoreResult::Resource(Option::Some(resource)))
            }
            ResourceStoreCommand::Get(identifier) => {
                let statement = match &identifier {
                    ResourceIdentifier::Key(_key) => {
                        "SELECT key,address,state_src,kind,specific,config_src FROM resources WHERE key=?1"
                    }
                    ResourceIdentifier::Address(_) => {
                        "SELECT key,address,state_src,kind,specific,config_src FROM resources WHERE address=?1"
                    }
                };

                let func = |row: &Row| {
                    let key: Vec<u8> = row.get(0)?;
                    let key = match ResourceKey::from_bin(key) {
                        Ok(key) => key,
                        Err(err) => {
                            return Err(rusqlite::Error::InvalidParameterName(err.to_string()));
                        }

                    };

                    let address: String = row.get(1)?;
                    let address = match ResourceAddress::from_str(address.as_str()) {
                        Ok(address) => address,
                        Err(error) => {
                            return Err(rusqlite::Error::InvalidParameterName(error.to_string()));
                        }
                    };

                    let state = if let ValueRef::Null = row.get_ref(2)? {
                        Option::None
                    } else {
                        let state: Vec<u8> = row.get(2)?;
                        Option::Some(state)
                    };

                    let kind: String = row.get(3)?;
                    let kind = match ResourceKind::from_str(kind.as_str()) {
                        Ok(kind) => kind,
                        Err(err) => {
                            return Err(rusqlite::Error::InvalidParameterName(err.to_string()));
                        }

                    };

                    let specific = if let ValueRef::Null = row.get_ref(4)? {
                        Option::None
                    } else {
                        let specific: String = row.get(4)?;
                        match Specific::from_str(specific.as_str()){
                            Ok(specific) => {
                                Option::Some(specific)
                            }
                            Err(err) => {
                                return Err(rusqlite::Error::InvalidParameterName(err.to_string()));
                            }
                        }
                    };

                    let config_src = if let ValueRef::Null = row.get_ref(5)? {
                        Option::None
                    } else {
                        let config_src: String = row.get(5)?;
                        let config_src = ConfigSrc::from_str(config_src.as_str())?;
                        Option::Some(config_src)
                    };

                    let state: DataSetSrc<LocalBinSrc> = match state {
                        None => {
                            DataSetSrc::new()
                        }
                        Some(state) => {
                            let bin = Arc::new(state);
                            let blob = DataSetBlob::from_bin(bin)?;
                            match blob.try_into() {
                                Ok(data_set) => data_set,
                                Err(err) => {
                                    error!("ERROR: {}",err.to_string());
                                    return Err(rusqlite::Error::InvalidQuery);
                                }
                            }
                        },
                    };

                    let archetype = ResourceArchetype {
                        kind: kind,
                        specific: specific,
                        config: config_src,
                    };

                    Ok(Resource::new(key, address, archetype, state))
                };

                let resource:rusqlite::Result<Resource> = match identifier.clone() {
                    ResourceIdentifier::Key(key) => {
                        let key = key.bin()?;
                        self.conn.query_row(statement, params![key], func)
                    }
                    ResourceIdentifier::Address(address) => {
                        self.conn
                            .query_row(statement, params![address.to_string()], func)
                    }
                };

                match resource {
                    Ok(resource) => Ok(ResourceStoreResult::Resource(Option::Some(resource))),
                    Err(err) => match err {
                        rusqlite::Error::QueryReturnedNoRows => {
                            Ok(ResourceStoreResult::Resource(Option::None))
                        }
                        _ => Err(err.to_string().into()),
                    },
                }
            }
        }
    }

    pub fn setup(&mut self) -> Result<(), Error> {
        let resources = r#"
       CREATE TABLE IF NOT EXISTS resources(
	      key BLOB PRIMARY KEY,
	      address TEXT NOT NULL,
	      state_src BLOB,
	      kind TEXT NOT NULL,
	      specific TEXT,
	      config_src TEXT,
	      UNIQUE(address)
        )"#;

        let transaction = self.conn.transaction()?;
        transaction.execute(resources, [])?;
        transaction.commit()?;

        Ok(())
    }
}
