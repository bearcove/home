use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct ItemCommon {
    pub id: String,
    #[serde(default)]
    pub relationships: HashMap<String, Relationship>,
}

#[derive(Deserialize, Debug)]
pub struct PatreonResponse {
    pub data: Vec<Item>,
    pub included: Vec<Item>,
    pub links: Option<Links>,
}

#[derive(Deserialize, Debug)]
pub struct Links {
    pub next: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Item {
    Member(Member),
    Tier(Tier),
}

#[derive(Deserialize, Debug)]
pub struct Member {
    #[serde(flatten)]
    pub common: ItemCommon,
    pub attributes: MemberAttributes,
}

impl Member {
    pub fn rel(&self, name: &str) -> Option<&Relationship> {
        self.common.relationships.get(name)
    }
}

#[derive(Deserialize, Debug)]
pub struct MemberAttributes {
    pub full_name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Tier {
    #[serde(flatten)]
    pub common: ItemCommon,
    pub attributes: TierAttributes,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TierAttributes {
    pub title: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Relationship {
    pub data: Vec<ItemRef>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ItemRef {
    Tier(TierRef),
}

#[derive(Deserialize, Debug, Clone)]
pub struct TierRef {
    pub id: String,
}
