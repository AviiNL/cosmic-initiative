use std::collections::HashSet;
use std::convert::{TryFrom, TryInto};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::iter::FromIterator;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use rusqlite::Connection;
use tempdir::TempDir;
use tokio::sync::{mpsc, Mutex};
use tracing_futures::WithSubscriber;

use starlane_resources::ResourceIdentifier;

use crate::core::{Host, StarCoreAction, StarCoreCommand};
use crate::error::Error;
use crate::file_access::{FileAccess, FileEvent};
use crate::message::Fail;
use crate::resource::{AddressCreationSrc, ArtifactBundleKind, AssignResourceStateSrc, DataTransfer, KeyCreationSrc, MemoryDataTransfer, Path, RemoteDataSrc, Resource, ResourceAddress, ResourceArchetype, ResourceAssign, ResourceCreate, ResourceCreateStrategy, ResourceCreationChamber, ResourceKey, ArtifactKind,ResourceKind, ResourceRecord, ResourceRegistration, ResourceRegistryInfo, ResourceStateSrc, ResourceStub, ResourceType};
use crate::resource::ArtifactBundleKey;
use crate::resource::store::{
    ResourceStore, ResourceStoreAction, ResourceStoreCommand, ResourceStoreResult,
};
use crate::star::StarSkel;
use crate::util;

pub struct ArtifactHost {
    skel: StarSkel,
    file_access: FileAccess,
    store: ResourceStore,
    mutex: Arc<Mutex<u8>>,
}

impl ArtifactHost {
    pub async fn new(skel: StarSkel, file_access: FileAccess) -> Result<Self, Error> {
        let mut file_access = file_access.with_path("bundles".to_string())?;
        let rtn = ArtifactHost {
            skel: skel,
            file_access: file_access,
            store: ResourceStore::new().await,
            mutex: Arc::new(Mutex::new(0)),
        };

        Ok(rtn)
    }


    fn bundle_key(key: ResourceKey) -> Result<ArtifactBundleKey, Fail> {
        let bundle_key = match key {
            ResourceKey::ArtifactBundle(key) => key,
            ResourceKey::Artifact(artifact) => artifact.bundle,
            key => {
                return Err(Fail::WrongResourceType {
                    expected: HashSet::from_iter(vec![
                        ResourceType::ArtifactBundle,
                        ResourceType::Artifact,
                    ]),
                    received: key.resource_type(),
                })
            }
        };

        Ok(bundle_key)
    }

    fn bundle_path(key: ResourceKey) -> Result<Path, Fail> {
        let bundle_key = Self::bundle_key(key)?;
        Ok(Path::from_str(
            format!("/{}", bundle_key.to_string().as_str()).as_str(),
        )?)
    }

    fn zip_bundle_path(key: ResourceKey) -> Result<Path, Fail> {
        let bundle_path = Self::bundle_path(key)?;
        Ok(bundle_path.cat(&Path::from_str("bundle.zip")?)?)
    }

    async fn ensure_bundle_dir(&mut self, key: ResourceKey) -> Result<(), Fail> {
        let path = Self::bundle_path(key)?;
        self.file_access.mkdir(&path).await?;
        Ok(())
    }

    fn validate(assign: &ResourceAssign<AssignResourceStateSrc>) -> Result<(), Fail> {
        Self::bundle_key(assign.key())?;
        Ok(())
    }

    async fn ensure_artifact(
        parent: ResourceStub,
        artifact_path: String,
        skel: StarSkel,
    ) -> Result<(), Error> {
        let archetype = ResourceArchetype {
            kind: ResourceKind::Artifact(ArtifactKind::Raw),
            specific: None,
            config: None,
        };

        let create = ResourceCreate {
            key: KeyCreationSrc::None,
            parent: parent.key.clone().into(),
            archetype: archetype,
            address: AddressCreationSrc::Append(artifact_path),
            src: AssignResourceStateSrc::Hosted,
            registry_info: Option::None,
            owner: Option::None,
            strategy: ResourceCreateStrategy::Ensure,
        };

        let rx = ResourceCreationChamber::new(parent, create, skel.clone()).await;

        let assign = util::wait_for_it_whatever(rx).await??;
        let stub = assign.stub.clone();
        let (action,mut rx) = StarCoreAction::new(StarCoreCommand::Assign(assign));
        skel.core_tx.send( action ).await;

        util::wait_for_it_whatever(rx).await??;

        let resource_record = ResourceRecord::new(stub, skel.info.key.clone() );
        let registration = ResourceRegistration::new(resource_record, Option::None );
        skel.registry.as_ref().expect("expected resource register to be available on ArtifactStore").register(registration).await;

        Ok(())
    }
}

#[async_trait]
impl Host for ArtifactHost {
    async fn assign(
        &mut self,
        assign: ResourceAssign<AssignResourceStateSrc>,
    ) -> Result<Resource, Fail> {

        Self::validate(&assign)?;

        // if there is Initialization to do for assignment THIS is where we do it
        self.ensure_bundle_dir(assign.stub.key.clone()).await?;

        //check for Final state violation
        if let Option::Some(resource) = self.store.get(assign.stub.address.clone().into()).await? {
            let kind: ArtifactBundleKind = assign.stub.address.clone().try_into()?;
            match kind {
                ArtifactBundleKind::Final => {
                    return Err(Fail::ResourceStateFinal(assign.stub.address.into()));
                }
                ArtifactBundleKind::Volatile => {
                    // delete old ArtifactBundle
                    self.delete(assign.stub.address.clone().into()).await?;
                }
            }
        }

        let artifacts = match assign.stub.key.resource_type() {
            ResourceType::ArtifactBundle => {
                match &assign.state_src {
                    AssignResourceStateSrc::Direct(data) => {
                        let artifacts = get_artifacts(data.clone())?;
                       let path = Self::bundle_path(assign.key())?;
                        let mut file_access = self.file_access.with_path(path.to_relative())?;
                        file_access
                            .write(&Path::new("/bundle.zip")?, data.clone())
                            .await?;

                        artifacts
                    }
                    AssignResourceStateSrc::Hosted => {
                        // do nothing, the file should already be present in the filesystem detected by the watcher and
                        // this call to assign is just making sure the database registry is updated
                        vec![]
                    }
                    AssignResourceStateSrc::None => {
                        return Err(Fail::Error(
                            "ArtifactBundle state should never be None".to_string(),
                        ));
                    }
                    AssignResourceStateSrc::InitArgs(_) => {
                        return Err(Fail::Error(
                            "ArtifactBundle cannot be created from InitArgs".to_string(),
                        ));
                    }
                }
            }
            ResourceType::Artifact => {
                vec![]
            }
            rt => {
                return Err(Fail::WrongResourceType {
                    expected: HashSet::from_iter(vec![
                        ResourceType::ArtifactBundle,
                        ResourceType::Artifact,
                    ]),
                    received: rt,
                });
            }
        };

        let data_transfer: Arc<dyn DataTransfer> = Arc::new(MemoryDataTransfer::none());

        let assign = ResourceAssign {
            stub: assign.stub.clone(),
            state_src: data_transfer,
        };

        let resource = self.store.put(assign).await?;
        {
            // at some point we need to ensure all of the artifacts but it must be AFTER
            // the registration for Bundle is fully commited...

            let skel = self.skel.clone();
            let parent: ResourceStub = resource.clone().into();
            tokio::spawn(async move {
                for artifact in artifacts {
                    let artifact = format!("/{}", artifact);
                    match Self::ensure_artifact(parent.clone(), artifact, skel.clone()).await{
                        Ok(_) => {}
                        Err(error) => {
                            error!("ensure artifact ERROR: {}",error.to_string());
                        }
                    }
                }
            });
        }

        Ok(resource)
    }

    async fn get(&self, identifier: ResourceIdentifier) -> Result<Option<Resource>, Fail> {
        self.store.get(identifier).await
    }

    async fn state(&self, identifier: ResourceIdentifier) -> Result<RemoteDataSrc, Fail> {
        if let Ok(Option::Some(resource)) = self.store.get(identifier.clone()).await {
            match identifier.resource_type() {
                ResourceType::File => {
                    let filesystem_key = resource
                        .key()
                        .parent()
                        .ok_or("Wheres the filesystem key?")?
                        .as_filesystem()?;
                    let filesystem_path =
                        Path::new(format!("/{}", filesystem_key.to_string().as_str()).as_str())?;
                    let path = format!(
                        "{}{}",
                        filesystem_path.to_string(),
                        resource.address().last_to_string()?
                    );
                    let data = self
                        .file_access
                        .read(&Path::from_str(path.as_str())?)
                        .await?;
                    Ok(RemoteDataSrc::Memory(data))
                }
                _ => Ok(RemoteDataSrc::None),
            }
        } else {
            Err(Fail::ResourceNotFound(identifier))
        }
    }

    async fn delete(&self, identifier: ResourceIdentifier) -> Result<(), Fail> {
        unimplemented!("I don't know how to DELETE yet.");
        Ok(())
    }

    fn shutdown(&self) {
        self.store.close();
        self.file_access.close();
    }

}

fn get_artifacts(data: Arc<Vec<u8>>) -> Result<Vec<String>, Fail> {
    let temp_dir = TempDir::new("zipcheck")?;

    let temp_path = temp_dir.path().clone();
    let file_path = temp_path.with_file_name("file.zip");
    let mut file = File::create(file_path.as_path())?;
    file.write_all(data.as_slice())?;

    let file = File::open(file_path.as_path())?;
    let mut archive = zip::ZipArchive::new(file);
    match archive {
        Ok(mut archive) => {
            let mut artifacts = vec![];
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                if !file.name().ends_with("/") {
                    artifacts.push(file.name().to_string())
                }
            }
            Ok(artifacts)
        }
        Err(error) => Err(Fail::InvalidResourceState(
            "ArtifactBundle must be a properly formatted Zip file.".to_string(),
        )),
    }
}

