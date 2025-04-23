use eyre::Result;
use jsonapi::model::{DocumentData, Resource};
use serde::de::DeserializeOwned;

pub trait BetterDoc {
    fn get_resource(&self, id: &str) -> Result<&Resource>;
}

impl BetterDoc for DocumentData {
    fn get_resource(&self, id: &str) -> Result<&Resource> {
        if let Some(resources) = self.included.as_ref() {
            if let Some(resource) = resources.iter().find(|res| res.id == id) {
                return Ok(resource);
            }
        }

        Err(eyre::eyre!("Could not find resource {id} in jsonapi doc"))
    }
}

pub trait BetterResource {
    fn get_single_relationship<'doc>(
        &self,
        doc: &'doc DocumentData,
        name: &str,
    ) -> Result<&'doc Resource>;

    fn get_multi_relationship<'doc>(
        &self,
        doc: &'doc DocumentData,
        name: &str,
    ) -> Result<Vec<&'doc Resource>>;

    fn get_attributes<T>(&self) -> Result<T>
    where
        T: DeserializeOwned;
}

impl BetterResource for Resource {
    fn get_single_relationship<'doc>(
        &self,
        doc: &'doc DocumentData,
        name: &str,
    ) -> Result<&'doc Resource> {
        if let Some(relationship) = self.get_relationship(name) {
            if let Ok(Some(id)) = relationship.as_id() {
                return doc.get_resource(id);
            }
        }

        Err(eyre::eyre!(
            "Could not get single relationship {} for {} in jsonapi doc",
            name,
            self._type
        ))
    }
    fn get_multi_relationship<'doc>(
        &self,
        doc: &'doc DocumentData,
        name: &str,
    ) -> Result<Vec<&'doc Resource>> {
        if let Some(relationship) = self.get_relationship(name) {
            if let Ok(Some(ids)) = relationship.as_ids() {
                return ids.iter().map(|id| doc.get_resource(id)).collect();
            }
        }

        Err(eyre::eyre!(
            "Could not get multi relationship {} for {} in jsonapi doc",
            name,
            self._type
        ))
    }

    fn get_attributes<T>(&self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let val = serde_json::to_value(&self.attributes)?;
        Ok(serde_json::from_value(val)?)
    }
}
