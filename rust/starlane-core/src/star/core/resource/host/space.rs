use std::convert::TryInto;
use std::sync::Arc;

use clap::{App, AppSettings};
use yaml_rust::Yaml;

use starlane_resources::{AssignKind, AssignResourceStateSrc, Resource, ResourceAssign};
use starlane_resources::data::{BinSrc, DataSet, Meta};
use starlane_resources::message::Fail;

use crate::artifact::ArtifactRef;
use crate::error::Error;
use crate::resource::{ArtifactKind, ResourceAddress, ResourceKey, ResourceType};
use crate::resource::create_args::{artifact_bundle_address, create_args_artifact_bundle, space_address};
use crate::star::core::resource::host::Host;
use crate::star::core::resource::state::StateStore;
use crate::star::StarSkel;

#[derive(Debug)]
pub struct SpaceHost {
    skel: StarSkel,
    store: StateStore,
}

impl SpaceHost {
    pub async fn new(skel: StarSkel) -> Self {
        SpaceHost {
            skel: skel.clone(),
            store: StateStore::new(skel).await,
        }
    }
}

#[async_trait]
impl Host for SpaceHost {
    async fn assign(
        &self,
        assign: ResourceAssign<AssignResourceStateSrc<DataSet<BinSrc>>>,
    ) -> Result<DataSet<BinSrc>, Error> {
        let state = match assign.state_src {
            AssignResourceStateSrc::Direct(data) => data,
            AssignResourceStateSrc::Stateless => return Err("space cannot be stateless".into()),
            AssignResourceStateSrc::CreateArgs(args) => {
                self.create_from_args(args).await?
            }
        };

        let assign = ResourceAssign {
            kind: assign.kind,
            stub: assign.stub,
            state_src: state,
        };

        Ok(self.store.put(assign).await?)
    }

    async fn has(&self, key: ResourceKey) -> bool {
        match self.store.has(key).await {
            Ok(v) => v,
            Err(_) => false,
        }
    }

    async fn get(&self, key: ResourceKey) -> Result<Option<DataSet<BinSrc>>, Error> {
        self.store.get(key).await
    }

    async fn delete(&self, _identifier: ResourceKey) -> Result<(), Error> {
        unimplemented!()
    }
}

impl SpaceHost {
    async fn create_from_args(&self, args: String) -> Result<DataSet<BinSrc>,Error> {

println!("SpaceHost: CREATE FROM ARGS...");
        let args:Vec<String> = args.trim().split(" ").map( |s| s.to_string()).collect();

        let factory = self.skel.machine.get_proto_artifact_caches_factory().await?;
        let mut cache = factory.create();
        let address = space_address()?;
        let artifact_ref = ArtifactRef {
            path: address.clone(),
            kind: ArtifactKind::Raw
        };
println!("SpaceHost: CACHING...");
        cache.cache(vec![artifact_ref]).await?;
println!("SpaceHost: CACHED...");
        let cache = cache.to_caches().await?;
        let yaml = cache.raw.get(&address ).ok_or("expected space.yaml")?;
        let yaml = yaml.data();
        let yaml = String::from_utf8((*yaml).clone() )?;
        let yaml = Yaml::from_str(yaml.as_str());

        let mut app = App::from_yaml( &yaml );

        let app = app.setting( AppSettings::NoBinaryName );
        let matches = app.get_matches_from(args);
        let display_name = matches.value_of("display-name".to_string() );
println!("DISPLAY NAME == '{}'", display_name.unwrap_or_default());

        let mut meta = Meta::new();
        meta.insert( "display-name".to_string(), display_name.ok_or("expected display nane")?.to_string() );
        let mut data_set = DataSet::new();
        let meta = BinSrc::Memory(Arc::new(meta.try_into()?));
        data_set.insert("meta".to_string(), meta );

        Ok(data_set)
    }
}