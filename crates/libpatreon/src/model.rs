use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PatreonResponse {
    pub data: Vec<Item>,
    pub included: Vec<Item>,
    pub links: Option<Links>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ItemCommon {
    pub id: String,
    #[serde(default)]
    pub relationships: Relationships,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Relationships {
    pub currently_entitled_tiers: Option<TierRelationship>,
    pub user: Option<UserRelationship>,
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
    User(User),
}

#[derive(Deserialize, Debug)]
pub struct Member {
    #[serde(flatten)]
    pub common: ItemCommon,
    pub attributes: MemberAttributes,
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
pub struct TierRelationship {
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

#[derive(Deserialize, Debug)]
pub struct User {
    #[serde(flatten)]
    pub common: ItemCommon,
    pub attributes: UserAttributes,
}

#[derive(Deserialize, Debug)]
pub struct UserAttributes {
    pub thumb_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UserRelationship {
    pub data: UserRef,
}

#[derive(Deserialize, Debug, Clone)]
pub struct UserRef {
    pub id: String,
}
