use crate::artifact::ArtRef;
use crate::config::mechtron::MechtronConfig;
use crate::{Bin, BindConfig, Point, SpaceErr, Stub};
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct ArtifactApi {
    binds: Arc<DashMap<Point, Arc<BindConfig>>>,
    mechtrons: Arc<DashMap<Point, Arc<MechtronConfig>>>,
    raw: Arc<DashMap<Point, Arc<Vec<u8>>>>,
    fetcher: Arc<dyn ArtifactFetcher>,
}

impl ArtifactApi {
    pub fn new(fetcher: Arc<dyn ArtifactFetcher>) -> Self {
        Self {
            binds: Arc::new(DashMap::new()),
            mechtrons: Arc::new(DashMap::new()),
            raw: Arc::new(DashMap::new()),
            fetcher,
        }
    }

    pub fn mechtron(&self, point: &Point) -> Result<ArtRef<MechtronConfig>, SpaceErr> {
        {
            if self.mechtrons.contains_key(point) {
                let mechtron = self.mechtrons.get(point).unwrap().clone();
                return Ok(ArtRef::new(mechtron, point.clone()));
            }
        }

        let mechtron: Arc<MechtronConfig> = Arc::new(self.fetch(point)?);
        self.mechtrons.insert(point.clone(), mechtron.clone());
        return Ok(ArtRef::new(mechtron, point.clone()));
    }

    pub fn bind(&self, point: &Point) -> Result<ArtRef<BindConfig>, SpaceErr> {
        {
            if self.binds.contains_key(point) {
                let bind = self.binds.get(point).unwrap().clone();
                return Ok(ArtRef::new(bind, point.clone()));
            }
        }

        let bind: Arc<BindConfig> = Arc::new(self.fetch(point)?);
        {
            self.binds.insert(point.clone(), bind.clone());
        }
        return Ok(ArtRef::new(bind, point.clone()));
    }


    pub fn raw(&self, point: &Point) -> Result<ArtRef<Vec<u8>>, SpaceErr> {
        if self.binds.contains_key(point) {
            let bin = self.raw.get(point).unwrap().clone();
            return Ok(ArtRef::new(bin, point.clone()));
        }

        let bin = self.fetcher.fetch(point)?;

        self.raw.insert( point.clone(), bin.clone() );

        return Ok(ArtRef::new(bin, point.clone()));
    }


    fn fetch<A>(&self, point: &Point) -> Result<A, SpaceErr>
    where
        A: TryFrom<Bin, Error = SpaceErr>,
    {
        if !point.has_bundle() {
            return Err("point is not from a bundle".into());
        }
        let bin = self.fetcher.fetch(point)?;
        Ok(A::try_from(bin)?)
    }
}
#[async_trait]
pub trait ArtifactFetcher: Send + Sync {
    fn stub(&self, point: &Point) -> Result<Stub, SpaceErr>;
    fn fetch(&self, point: &Point) -> Result<Bin, SpaceErr>;
}
