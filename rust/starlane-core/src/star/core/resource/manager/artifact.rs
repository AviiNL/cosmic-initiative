use std::cmp::Ordering;
use std::collections::{HashSet, HashMap};
use std::convert::TryInto;
use std::fs::File;
use std::io::Write;
use std::iter::FromIterator;
use std::str::FromStr;
use std::sync::Arc;

use tempdir::TempDir;
use tokio::sync::Mutex;

use crate::resource::{ResourceType, AssignResourceStateSrc, ResourceAssign};
use crate::star::core::resource::manager::ResourceManager;
use crate::star::core::resource::state::StateStore;
use crate::star::StarSkel;
use crate::util;
use crate::error::Error;

use crate::message::delivery::Delivery;
use mesh_portal_serde::version::latest::command::common::StateSrc;
use mesh_portal_serde::version::latest::entity::request::create::{AddressSegmentTemplate, AddressTemplate, Create, Strategy, Template};
use mesh_portal_serde::version::latest::entity::request::{Rc, RcCommand};
use mesh_portal_serde::version::latest::id::{Address, AddressAndKind, KindParts, RouteSegment};
use mesh_portal_serde::version::latest::messaging::Request;
use mesh_portal_serde::version::latest::payload::{Payload, Primitive};
use mesh_portal_versions::version::v0_0_1::entity::request::create::KindTemplate;
use mesh_portal_versions::version::v0_0_1::entity::request::ReqEntity;
use mesh_portal_versions::version::v0_0_1::entity::response::RespEntity;
use crate::file_access::FileAccess;


fn get_artifacts(data: Arc<Vec<u8>>) -> Result<Vec<String>, Error> {
    let temp_dir = TempDir::new("zipcheck")?;
    let temp_path = temp_dir.path().clone();
    let file_path = temp_path.with_file_name("file.zip");
    let mut file = File::create(file_path.as_path())?;
    file.write_all(data.as_slice())?;

    let file = File::open(file_path.as_path())?;
    let archive = zip::ZipArchive::new(file);
    match archive {
        Ok(mut archive) => {
            let mut artifacts = vec![];
            for i in 0..archive.len() {
                let file = archive.by_index(i).unwrap();
                if !file.name().ends_with("/") {
                    artifacts.push(file.name().to_string())
                }
            }
            Ok(artifacts)
        }
        Err(_error) => Err(
            "ArtifactBundle must be a properly formatted Zip file.".into(),
        ),
    }
}

#[derive(Debug)]
pub struct ArtifactBundleManager {
    skel: StarSkel,
    store: StateStore,
}

impl ArtifactBundleManager {
    pub async fn new(skel: StarSkel) -> Self {
        ArtifactBundleManager {
            skel: skel.clone(),
            store: StateStore::new(skel),
        }
    }
}

#[async_trait]
impl ResourceManager for ArtifactBundleManager {
    fn resource_type(&self) -> ResourceType {
        ResourceType::ArtifactBundle
    }

    async fn assign(
        &self,
        assign: ResourceAssign,
    ) -> Result<(), Error> {
        let state = match &assign.state {
            StateSrc::StatefulDirect(data) => {
                data.clone()
            },
            StateSrc::Stateless => {
                return Err("ArtifactBundle cannot be stateless".into())
            },

        };

println!("$??????? ASSIGNING ARTIFACT BUNDLE!!!!");
        if let Payload::Primitive( Primitive::Bin(zip) ) = state.clone() {
            let artifacts = get_artifacts(zip)?;
            let mut address_and_kind_set = HashSet::new();
            for artifact in artifacts {
                let mut path = String::new();
                let segments = artifact.split("/");
                let segments :Vec<&str> = segments.collect();
                for (index,segment) in segments.iter().enumerate() {
                    path.push_str(segment);
                    if index < segments.len()-1 {
                        path.push_str("/");
                    }
                    let address = Address::from_str( format!( "{}:/{}",assign.stub.address.to_string(), path.as_str()).as_str() )?;
                    let kind = if index < segments.len()-1 {
                        KindParts { resource_type: "Artifact".to_string(), kind: Option::Some("Dir".to_string()), specific: None }
                    }  else {
                        KindParts { resource_type: "Artifact".to_string(), kind: Option::Some("Raw".to_string()), specific: None }
                    };
                    let address_and_kind = AddressAndKind {
                        address,
                        kind
                    };
                    address_and_kind_set.insert( address_and_kind );
                }

            }

            let root_address_and_kind = AddressAndKind {
               address: Address::from_str( format!( "{}:/",assign.stub.address.to_string()).as_str())?,
               kind: KindParts { resource_type: "Artifact".to_string(), kind: Option::Some("Dir".to_string()), specific: None }
            };

println!("?~ ROOT: {}", root_address_and_kind.address.to_string() );

            address_and_kind_set.insert( root_address_and_kind );

            let mut address_and_kind_set: Vec<AddressAndKind>  = address_and_kind_set.into_iter().collect();

            // shortest first will ensure that dirs are created before files
            address_and_kind_set.sort_by(|a,b|{
                if a.address.to_string().len() > b.address.to_string().len() {
                    Ordering::Greater
                } else if a.address.to_string().len() < b.address.to_string().len() {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            });
            for address_and_kind in &address_and_kind_set {
                println!("?~ ARTIFACT ADDRESS: {}", address_and_kind.address.to_string() );
            }

            {
                let skel = self.skel.clone();
                let assign = assign.clone();
                tokio::spawn(async move {
                    for address_and_kind in address_and_kind_set {
                        println!("~~ ARTIFACT ADDRESS: {}", address_and_kind.address.to_string());
                        println!("... last seg {}", address_and_kind.address.last_segment().expect("expected final segment").to_string());
                        let parent = address_and_kind.address.parent().expect("expected parent");

                        println!("... parent seg {}", parent.to_string());
                        let create = Create {
                            template: Template {
                                address: AddressTemplate { parent: parent.clone(), child_segment_template: AddressSegmentTemplate::Exact(address_and_kind.address.last_segment().expect("expected final segment").to_string()) },
                                kind: KindTemplate { resource_type: address_and_kind.kind.resource_type.clone(), kind: address_and_kind.kind.kind.clone(), specific: None }
                            },
                            state: StateSrc::Stateless,
                            properties: vec![],
                            strategy: Strategy::Create,
                            registry: Default::default()
                        };

                        println!("SENDING REQUEST TO PARENT: {}", parent.to_string());
                        let request = Request::new(ReqEntity::Rc(Rc::empty_payload(RcCommand::Create(create))), assign.stub.address.clone(), parent);
                        let response = skel.messaging_api.exchange(request).await;
                        match response {
                            Ok(response) => {
                                match response.entity {
                                    RespEntity::Ok(_) => {
                                        println!("added artifact: {}", address_and_kind.address.to_string());
                                    }
                                    RespEntity::Fail(_) => {
                                        println!("FAILED to add artifact: {}", address_and_kind.address.to_string());
                                    }
                                }
                            }
                            _ => {
                                println!("unexpected result");
                            }
                        }
                    }
                });
            }
        }

        self.store.put( assign.stub.address, state ).await?;

        // need to unzip and create Artifacts for each...



        Ok(())
    }


    async fn has(&self, address: Address) -> bool {
        match self.store.has(address).await {
            Ok(v) => v,
            Err(_) => false,
        }
    }

}


pub struct ArtifactManager {

}

impl ArtifactManager {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl ResourceManager for ArtifactManager{
    fn resource_type(&self) -> ResourceType {
        ResourceType::Artifact
    }

    async fn assign(&self, assign: ResourceAssign) -> Result<(), Error> {
        Ok(())
    }

    async fn has(&self, address: Address) -> bool {
        false
    }
}
