#![feature(structural_match)]
#![feature(box_syntax)]
#![feature(derive_eq)]
#![feature(fmt_internals)]

use std::convert::TryFrom;
use std::convert::TryInto;
use std::str::FromStr;

use nom::error::{context, ErrorKind, ParseError, VerboseError};
use serde::de::Error as OtherError;
use serde::{Deserialize, Serialize};

use starlane_core::error::Error;
use starlane_core::resource::address::parse_address;
use starlane_core::resource::address::KeyBit;
use starlane_core::resource::address::KeyBits;
use starlane_core::resource::address::ParentAddressPatternRecognizer;
use starlane_core::resource::address::Res;
use starlane_core::resource::address::ResourceAddressPart;
use starlane_core::resource::address::ResourceKindParts;
use starlane_core::resource::address::Specific;
use starlane_core::star::StarKind;
use starlane_macros::resources;
use starlane_core::resource::ResourceAddressPart;

pub fn parse_address_part(string: &str) -> Result<(&str, Vec<ResourceAddressPart>), Error> {
    unimplemented!()
}

fn main() {
    println!("Hello, world!");
}

resources! {


#[resource(parents(Root))]
#[resource(stars(SpaceHost))]
#[resource(prefix="spc")]
#[resource(ResourceAddressPartKind::SkewerCase)]
pub struct Space();

#[resource(parents(Space))]
#[resource(stars(SpaceHost))]
#[resource(prefix="sub")]
#[resource(ResourceAddressPartKind::SkewerCase)]
pub struct SubSpace();


#[resource(parents(SubSpace))]
#[resource(stars(AppHost))]
#[resource(prefix="app")]
#[resource(ResourceAddressPartKind::SkewerCase)]
pub struct App();

#[resource(parents(SubSpace,App))]
#[resource(stars(SpaceHost,AppHost))]
#[resource(prefix="db")]
#[resource(ResourceAddressPartKind::SkewerCase)]
pub struct Database();

#[derive(Clone,Debug,Eq,PartialEq,Hash,Serialize,Deserialize)]
pub enum DatabaseKind{
    Native,
    External(Specific)
}




}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use starlane_core::error::Error;
    use starlane_core::resource::address::Specific;

    use crate::DatabaseKind;
    use crate::ResourceKind;

    #[test]
    fn space_key() -> Result<(), Error> {
        let kind = DatabaseKind::Native;
        let kind2 = DatabaseKind::External(Specific::from_str("mysql.org:mysql:innodb:1.0.0")?);
        let kind: ResourceKind = kind.into();
        let kind2: ResourceKind = kind2.into();
        println!("{}", kind.to_string());
        println!("{}", kind2.to_string());
        Ok(())
    }
}
